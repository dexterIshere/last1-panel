//! Icône de Last1 Panel près de l'horloge (StatusNotifierItem : Plasma,
//! Waybar sous Hyprland, GNOME avec l'extension AppIndicator), tenue par le
//! service de la session : profils de couleurs mis sur l'écran d'un clic,
//! fenêtre de Last1 Panel ouverte. Masquée, elle revient par la page
//! « Couleurs » (réglage `tray`).

use crate::color::curve::ColorAdjust;
use crate::i18n::t;
use crate::panel::Panel;
use crate::settings::Settings;
use crate::system::desktop;
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

/// Ce que le menu demande au service (rien de lent dans le menu lui-même).
#[derive(Debug, Clone, PartialEq)]
enum Request {
    Open,
    /// `None` : valeurs par défaut.
    Apply {
        screens: Vec<String>,
        profile: Option<String>,
    },
    Hide,
    ScreensChanged,
}

/// Prévient l'icône d'un écran branché ou débranché.
#[derive(Clone)]
pub struct Notifier(Sender<Request>);

impl Notifier {
    pub fn screens_changed(&self) {
        let _ = self.0.send(Request::ScreensChanged);
    }
}

/// Couleurs d'un écran, d'après les réglages enregistrés.
#[derive(Debug, Clone, PartialEq)]
enum Active {
    Default,
    Profile(String),
    /// Réglé à la main, sans profil.
    Custom,
}

#[derive(Debug, Clone, PartialEq)]
struct ScreenModel {
    name: String,
    label: String,
    active: Active,
}

/// Ce que montre le menu.
#[derive(Debug, Clone, Default, PartialEq)]
struct Model {
    /// Identifiant et nom.
    profiles: Vec<(String, String)>,
    screens: Vec<ScreenModel>,
    can_open: bool,
}

fn model(settings: &Settings, screens: &[(String, String)]) -> Model {
    Model {
        profiles: settings.color_profiles.iter().map(|p| (p.id.clone(), p.name.clone())).collect(),
        screens: screens
            .iter()
            .map(|(name, label)| ScreenModel {
                name: name.clone(),
                label: label.clone(),
                active: match settings.active_profile(name) {
                    Some(None) => Active::Default,
                    Some(Some(profile)) => Active::Profile(profile.id.clone()),
                    None => Active::Custom,
                },
            })
            .collect(),
        can_open: panel_app().is_some(),
    }
}

struct Icon {
    model: Model,
    requests: Sender<Request>,
}

impl Icon {
    fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}

/// Icône en pixels, si le thème d'icônes n'a pas « last1-panel ».
static PIXMAP: LazyLock<Vec<ksni::Icon>> = LazyLock::new(|| {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/tray.png")));
    let Ok(mut reader) = decoder.read_info() else { return Vec::new() };
    let mut rgba = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let Ok(info) = reader.next_frame(&mut rgba) else { return Vec::new() };
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Vec::new();
    }
    // ARGB32, octets de poids fort en premier.
    let data = rgba[..info.buffer_size()].chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    vec![ksni::Icon { width: info.width as i32, height: info.height as i32, data }]
});

/// Les menus D-Bus soulignent la lettre qui suit « _ » : « Arc_Raiders »
/// s'afficherait « ArcRaiders ».
fn label(text: &str) -> String {
    text.replace('_', "__")
}

impl ksni::Tray for Icon {
    fn id(&self) -> String {
        "last1-panel".into()
    }

    fn title(&self) -> String {
        "Last1 Panel".into()
    }

    fn icon_name(&self) -> String {
        "last1-panel".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        PIXMAP.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let lines: Vec<String> = self
            .model
            .screens
            .iter()
            .map(|screen| {
                let colors = match &screen.active {
                    Active::Default => t("couleurs par défaut", "default colors"),
                    Active::Profile(id) => self
                        .model
                        .profiles
                        .iter()
                        .find(|(p, _)| p == id)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default(),
                    Active::Custom => t("réglage personnel", "custom setting"),
                };
                format!("{} : {colors}", screen.label)
            })
            .collect();
        ksni::ToolTip { title: "Last1 Panel".into(), description: lines.join("\n"), ..Default::default() }
    }

    /// Clic gauche : la fenêtre ; le menu est au clic droit.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Request::Open);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        menu(&self.model)
    }

    /// Pas encore de zone de notification (bureau en train de s'ouvrir, ou
    /// GNOME sans l'extension) : l'icône y entrera quand elle apparaîtra.
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        true
    }
}

/// Choix des couleurs d'un écran : valeurs par défaut, puis les profils.
fn colors_choice(model: &Model, screen: &ScreenModel) -> MenuItem<Icon> {
    let mut options = vec![RadioItem { label: t("Valeurs par défaut", "Defaults"), ..Default::default() }];
    options.extend(model.profiles.iter().map(|(_, name)| RadioItem { label: label(name), ..Default::default() }));
    let selected = match &screen.active {
        Active::Default => 0,
        Active::Profile(id) => model.profiles.iter().position(|(p, _)| p == id).map_or(usize::MAX, |i| i + 1),
        Active::Custom => {
            options.push(RadioItem {
                label: t("Réglage personnel", "Custom setting"),
                enabled: false,
                ..Default::default()
            });
            options.len() - 1
        }
    };
    let ids: Vec<Option<String>> =
        std::iter::once(None).chain(model.profiles.iter().map(|(id, _)| Some(id.clone()))).collect();
    let output = screen.name.clone();
    RadioGroup {
        selected,
        select: Box::new(move |icon: &mut Icon, index| {
            let Some(profile) = ids.get(index) else { return };
            // Coché tout de suite ; le service corrige si l'écran refuse.
            if let Some(screen) = icon.model.screens.iter_mut().find(|s| s.name == output) {
                screen.active = profile.clone().map_or(Active::Default, Active::Profile);
            }
            icon.send(Request::Apply { screens: vec![output.clone()], profile: profile.clone() });
        }),
        options,
    }
    .into()
}

fn menu(model: &Model) -> Vec<MenuItem<Icon>> {
    let mut items: Vec<MenuItem<Icon>> = vec![
        StandardItem {
            label: t("Ouvrir Last1 Panel", "Open Last1 Panel"),
            icon_name: "last1-panel".into(),
            enabled: model.can_open,
            activate: Box::new(|icon: &mut Icon| icon.send(Request::Open)),
            ..Default::default()
        }
        .into(),
        MenuItem::Separator,
    ];
    match model.screens.as_slice() {
        [] => items.push(
            StandardItem {
                label: t("Aucun écran à régler", "No screen to set"), enabled: false, ..Default::default()
            }
            .into(),
        ),
        [screen] => {
            items.push(
                StandardItem {
                    label: t("Couleurs de l'écran", "Screen colors"), enabled: false, ..Default::default()
                }
                .into(),
            );
            items.push(colors_choice(model, screen));
        }
        screens => {
            for screen in screens {
                items.push(
                    SubMenu {
                        label: label(&format!("{} ({})", screen.label, screen.name)),
                        submenu: vec![colors_choice(model, screen)],
                        ..Default::default()
                    }
                    .into(),
                );
            }
            let all: Vec<String> = screens.iter().map(|s| s.name.clone()).collect();
            let choices = std::iter::once((None, t("Valeurs par défaut", "Defaults")))
                .chain(model.profiles.iter().map(|(id, name)| (Some(id.clone()), label(name))));
            items.push(
                SubMenu {
                    label: t("Tous les écrans", "All screens"),
                    submenu: choices
                        .map(|(profile, text)| {
                            let screens = all.clone();
                            StandardItem {
                                label: text,
                                activate: Box::new(move |icon: &mut Icon| {
                                    icon.send(Request::Apply { screens: screens.clone(), profile: profile.clone() })
                                }),
                                ..Default::default()
                            }
                            .into()
                        })
                        .collect(),
                    ..Default::default()
                }
                .into(),
            );
        }
    }
    if model.profiles.is_empty() {
        items.push(
            StandardItem {
                label: t("Vos profils se créent dans Couleurs", "Create your profiles in Colors"),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
    }
    items.push(MenuItem::Separator);
    items.push(
        StandardItem {
            label: t("Masquer cette icône", "Hide this icon"),
            activate: Box::new(|icon: &mut Icon| icon.send(Request::Hide)),
            ..Default::default()
        }
        .into(),
    );
    items
}

/// Last1 Panel installé : par le paquet, sinon l'AppImage.
fn panel_app() -> Option<PathBuf> {
    [PathBuf::from(crate::paths::SYSTEM_APP), crate::paths::user_appimage()].into_iter().find(|path| path.is_file())
}

/// Fenêtre ouverte hors du service (systemd-run) : elle reste ouverte quand
/// le service redémarre après une mise à jour.
fn open_panel() {
    let Some(app) = panel_app() else { return };
    let started = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .arg(&app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !started {
        let child = Command::new(&app)
            .envs(desktop::session_env())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        if let Ok(mut child) = child {
            std::thread::spawn(move || child.wait());
        }
    }
}

/// Couleurs d'un profil sur ces écrans ; ce qui échoue est notifié.
fn apply(screens: &[String], profile: Option<&str>) {
    let settings = Settings::load();
    let (color, vibrance) = match profile {
        None => (ColorAdjust::default(), 50.0),
        Some(id) => match settings.color_profile(id) {
            Some(profile) => (profile.color, profile.vibrance),
            None => return,
        },
    };
    let panel = Panel::session();
    let problems: Vec<String> = screens
        .iter()
        .filter_map(|screen| panel.apply_colors(screen, &color, vibrance).err().map(|e| e.to_string()))
        .collect();
    if !problems.is_empty() {
        desktop::notify(&problems.join("\n"));
    }
}

/// Écrans allumés : connecteur et nom.
fn screens() -> Vec<(String, String)> {
    Panel::session()
        .outputs()
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.enabled)
        .map(|o| {
            let label = if o.label.trim().is_empty() { o.name.clone() } else { o.label.trim().to_string() };
            (o.name, label)
        })
        .collect()
}

/// Icône tenue par un fil du service ; elle suit les réglages (profils
/// créés dans la fenêtre, icône masquée ou remise) en les relisant toutes
/// les 2 s s'ils ont changé.
pub fn spawn() -> Notifier {
    let (sender, requests) = mpsc::channel();
    let notifier = Notifier(sender.clone());
    std::thread::spawn(move || serve(sender, requests));
    notifier
}

fn settings_stamp() -> Option<SystemTime> {
    std::fs::metadata(Settings::path()).and_then(|m| m.modified()).ok()
}

fn serve(sender: Sender<Request>, requests: Receiver<Request>) {
    let mut handle: Option<ksni::blocking::Handle<Icon>> = None;
    let mut stamp = None;
    let mut screens = Vec::new();
    // Écrans relus quand ils changent, et tant qu'aucun n'est lu (le bureau
    // s'ouvre encore).
    let mut screens_read = Instant::now() - Duration::from_secs(3600);
    // D-Bus a refusé l'icône : nouvel essai 30 s plus tard.
    let mut retry_at: Option<Instant> = None;
    let mut current = Model::default();
    loop {
        let mut fresh = false;
        match requests.recv_timeout(Duration::from_secs(2)) {
            Ok(Request::Open) => open_panel(),
            Ok(Request::Apply { screens, profile }) => {
                apply(&screens, profile.as_deref());
                fresh = true;
            }
            Ok(Request::Hide) => {
                if Settings::update(&Settings::path(), |s| s.tray = false).is_ok() {
                    desktop::notify(&t(
                        "Icône masquée : elle revient par Last1 Panel, page Couleurs.",
                        "Icon hidden: bring it back from Last1 Panel, Colors page.",
                    ));
                }
            }
            Ok(Request::ScreensChanged) => screens_read = Instant::now() - Duration::from_secs(3600),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now_stamp = settings_stamp();
        if now_stamp != stamp {
            stamp = now_stamp;
            fresh = true;
        }
        if screens_read.elapsed() > Duration::from_secs(if screens.is_empty() { 10 } else { 3600 }) {
            screens = self::screens();
            screens_read = Instant::now();
            fresh = true;
        }
        let retry = retry_at.is_some_and(|at| at <= Instant::now());
        if !fresh && !retry {
            continue;
        }
        let settings = Settings::load();
        let next = model(&settings, &screens);
        match (&handle, settings.tray) {
            (Some(icon), true) => {
                if next != current {
                    icon.update(|icon| icon.model = next.clone());
                }
            }
            (Some(icon), false) => {
                icon.shutdown().wait();
                handle = None;
            }
            (None, true) if retry_at.is_none_or(|at| at <= Instant::now()) => {
                match (Icon { model: next.clone(), requests: sender.clone() }).assume_sni_available(true).spawn() {
                    Ok(icon) => {
                        handle = Some(icon);
                        retry_at = None;
                    }
                    Err(error) => {
                        eprintln!("last1-panel-cli : icône : {error}");
                        retry_at = Some(Instant::now() + Duration::from_secs(30));
                    }
                }
            }
            _ => {}
        }
        current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::curve::Channel;

    fn labels(items: &[MenuItem<Icon>]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Standard(item) => item.label.clone(),
                MenuItem::SubMenu(menu) => format!("{} >", menu.label),
                MenuItem::RadioGroup(group) => group
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("{}{}", if i == group.selected { "(x) " } else { "( ) " }, o.label))
                    .collect::<Vec<_>>()
                    .join(" | "),
                MenuItem::Separator => "--".into(),
                _ => "?".into(),
            })
            .collect()
    }

    #[test]
    fn menu_shows_the_profile_of_each_screen() {
        let mut settings = Settings::default();
        let guide = ColorAdjust::all(Channel { brightness: 52.0, contrast: 47.0, gamma: 1.47 });
        settings.put_color_profile(None, "Arc_Raiders", guide, 62.0);
        let screen = settings.outputs.entry("DP-1".into()).or_default();
        screen.color = guide;
        screen.vibrance = Some(62.0);

        let one = model(&settings, &[("DP-1".into(), "AW2723DF".into())]);
        assert_eq!(labels(&menu(&one))[2..4], ["Couleurs de l'écran", "( ) Valeurs par défaut | (x) Arc__Raiders"]);

        // Deuxième écran réglé à la main : un sous-menu par écran.
        settings.outputs.entry("HDMI-A-1".into()).or_default().vibrance = Some(80.0);
        let two = model(&settings, &[("DP-1".into(), "AW2723DF".into()), ("HDMI-A-1".into(), "LG".into())]);
        assert_eq!(two.screens[1].active, Active::Custom);
        let items = menu(&two);
        assert_eq!(labels(&items)[2..5], ["AW2723DF (DP-1) >", "LG (HDMI-A-1) >", "Tous les écrans >"]);
        let MenuItem::SubMenu(lg) = &items[3] else { panic!() };
        assert_eq!(labels(&lg.submenu), ["( ) Valeurs par défaut | ( ) Arc__Raiders | (x) Réglage personnel"]);
    }

    #[test]
    fn menu_without_profiles_says_where_to_make_them() {
        let items = labels(&menu(&model(&Settings::default(), &[("DP-1".into(), "DP-1".into())])));
        assert!(items.contains(&"(x) Valeurs par défaut".to_string()));
        assert!(items.contains(&"Vos profils se créent dans Couleurs".to_string()));
        assert_eq!(items.last().unwrap(), "Masquer cette icône");
    }

    #[test]
    fn pixmap_is_argb() {
        let icon = &PIXMAP[0];
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.data.len(), 64 * 64 * 4);
    }
}
