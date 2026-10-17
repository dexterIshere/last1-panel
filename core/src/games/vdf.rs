//! Format texte KeyValues de Valve (libraryfolders.vdf, appmanifest_*.acf,
//! localconfig.vdf). Lu en arbre ordonné, réécrit comme Steam l'écrit : le
//! fichier relu par Steam ne diffère que par ce qu'on a changé.

use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Block(Block),
}

/// Bloc `{ … }` : clés dans l'ordre du fichier (Steam tolère les doublons).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub entries: Vec<(String, Value)>,
}

impl Block {
    /// Valeur de `key`, sans tenir compte de la casse (Steam écrit tantôt
    /// « apps », tantôt « Apps »).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)).map(|(_, value)| value)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Text(text) => Some(text),
            Value::Block(_) => None,
        }
    }

    pub fn block(&self, key: &str) -> Option<&Block> {
        match self.get(key)? {
            Value::Block(block) => Some(block),
            Value::Text(_) => None,
        }
    }

    /// Bloc au bout d'un chemin de clés.
    pub fn path(&self, keys: &[&str]) -> Option<&Block> {
        keys.iter().try_fold(self, |block, key| block.block(key))
    }

    /// Bloc `key`, créé (à la fin) s'il manque ou s'il était un texte.
    pub fn block_mut(&mut self, key: &str) -> &mut Block {
        let index = match self.entries.iter().position(|(name, _)| name.eq_ignore_ascii_case(key)) {
            Some(index) => {
                if !matches!(self.entries[index].1, Value::Block(_)) {
                    self.entries[index].1 = Value::Block(Block::default());
                }
                index
            }
            None => {
                self.entries.push((key.to_string(), Value::Block(Block::default())));
                self.entries.len() - 1
            }
        };
        match &mut self.entries[index].1 {
            Value::Block(block) => block,
            Value::Text(_) => unreachable!("remplacé par un bloc ci-dessus"),
        }
    }

    pub fn path_mut(&mut self, keys: &[&str]) -> &mut Block {
        keys.iter().fold(self, |block, key| block.block_mut(key))
    }

    /// Donne la valeur texte `value` à `key` (à sa place si elle existe).
    pub fn set_text(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(key)) {
            Some((_, slot)) => *slot = Value::Text(value.to_string()),
            None => self.entries.push((key.to_string(), Value::Text(value.to_string()))),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(name, _)| !name.eq_ignore_ascii_case(key));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VdfError {
    #[error("VDF : guillemet non fermé")]
    UnclosedQuote,
    #[error("VDF : bloc sans clé")]
    BlockWithoutKey,
    #[error("VDF : accolade fermante en trop")]
    UnexpectedClose,
    #[error("VDF : accolade non fermée")]
    UnclosedBlock,
    #[error("VDF : clé « {0} » sans valeur")]
    KeyWithoutValue(String),
}

enum Token {
    Text(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Result<Vec<Token>, VdfError> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut value = String::new();
                loop {
                    match chars.next().ok_or(VdfError::UnclosedQuote)? {
                        '"' => break,
                        '\\' => match chars.next().ok_or(VdfError::UnclosedQuote)? {
                            'n' => value.push('\n'),
                            't' => value.push('\t'),
                            other => value.push(other),
                        },
                        other => value.push(other),
                    }
                }
                out.push(Token::Text(value));
            }
            _ => {
                let mut value = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || next == '{' || next == '}' || next == '"' {
                        break;
                    }
                    value.push(next);
                    chars.next();
                }
                // Conditions de plateforme ([$WIN32]…) : ignorées, comme Steam sous Linux.
                if !(value.starts_with('[') && value.ends_with(']')) {
                    out.push(Token::Text(value));
                }
            }
        }
    }
    Ok(out)
}

pub fn parse(text: &str) -> Result<Block, VdfError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut stack: Vec<(String, Block)> = Vec::new();
    let mut current = Block::default();
    let mut pending: Option<String> = None;
    for token in tokens(text)? {
        match token {
            Token::Text(text) => match pending.take() {
                None => pending = Some(text),
                Some(key) => current.entries.push((key, Value::Text(text))),
            },
            Token::Open => {
                let key = pending.take().ok_or(VdfError::BlockWithoutKey)?;
                stack.push((key, std::mem::take(&mut current)));
            }
            Token::Close => {
                if let Some(key) = pending.take() {
                    return Err(VdfError::KeyWithoutValue(key));
                }
                let (key, mut parent) = stack.pop().ok_or(VdfError::UnexpectedClose)?;
                parent.entries.push((key, Value::Block(std::mem::take(&mut current))));
                current = parent;
            }
        }
    }
    if !stack.is_empty() {
        return Err(VdfError::UnclosedBlock);
    }
    if let Some(key) = pending {
        return Err(VdfError::KeyWithoutValue(key));
    }
    Ok(current)
}

/// Texte dans le format de Steam : tabulations, deux tabulations entre clé et
/// valeur, accolades sur leur ligne.
pub fn serialize(root: &Block) -> String {
    let mut out = String::new();
    write_block(&mut out, root, 0);
    out
}

fn write_block(out: &mut String, block: &Block, depth: usize) {
    let indent = "\t".repeat(depth);
    for (key, value) in &block.entries {
        match value {
            Value::Text(text) => {
                let _ = writeln!(out, "{indent}\"{}\"\t\t\"{}\"", escape(key), escape(text));
            }
            Value::Block(child) => {
                let _ = writeln!(out, "{indent}\"{}\"", escape(key));
                let _ = writeln!(out, "{indent}{{");
                write_block(out, child, depth + 1);
                let _ = writeln!(out, "{indent}}}");
            }
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARIES: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/joueur/.local/share/Steam"
		"label"		""
		"apps"
		{
			"228980"		"233052342"
		}
	}
	"1"
	{
		"path"		"/mnt/windows-d/SteamLibrary"
		"label"		"Jeux"
		"apps"
		{
			"1091500"		"70523498135"
		}
	}
}
"#;

    #[test]
    fn reads_nested_blocks() {
        let root = parse(LIBRARIES).unwrap();
        let folders = root.block("LibraryFolders").unwrap();
        assert_eq!(folders.path(&["1"]).unwrap().text("path"), Some("/mnt/windows-d/SteamLibrary"));
        assert_eq!(folders.path(&["1", "apps"]).unwrap().text("1091500"), Some("70523498135"));
    }

    #[test]
    fn writes_back_identically() {
        assert_eq!(serialize(&parse(LIBRARIES).unwrap()), LIBRARIES);
    }

    #[test]
    fn escapes_survive_a_round_trip() {
        let text = "\"apps\"\n{\n\t\"570\"\n\t{\n\t\t\"LaunchOptions\"\t\t\"PROTON_LOG=1 \\\"%command%\\\" -path C:\\\\Jeux\"\n\t}\n}\n";
        let root = parse(text).unwrap();
        assert_eq!(
            root.path(&["apps", "570"]).unwrap().text("LaunchOptions"),
            Some(r#"PROTON_LOG=1 "%command%" -path C:\Jeux"#)
        );
        assert_eq!(serialize(&root), text);
    }

    #[test]
    fn comments_conditions_and_bare_words() {
        let root = parse("// commentaire\n\"a\" { \"b\" \"1\" [$WIN32] c 2 }").unwrap();
        let a = root.block("a").unwrap();
        assert_eq!(a.text("b"), Some("1"));
        assert_eq!(a.text("c"), Some("2"));
    }

    #[test]
    fn edits_keep_other_entries() {
        let mut root = parse(LIBRARIES).unwrap();
        root.path_mut(&["libraryfolders", "1"]).set_text("label", "Disque D");
        root.path_mut(&["libraryfolders", "2"]).set_text("path", "/nouveau");
        let again = parse(&serialize(&root)).unwrap();
        let folders = again.block("libraryfolders").unwrap();
        assert_eq!(folders.path(&["1"]).unwrap().text("label"), Some("Disque D"));
        assert_eq!(folders.path(&["1"]).unwrap().text("path"), Some("/mnt/windows-d/SteamLibrary"));
        assert_eq!(folders.path(&["2"]).unwrap().text("path"), Some("/nouveau"));
        assert_eq!(folders.entries.len(), 3);
    }

    #[test]
    fn invalid_files_are_refused() {
        assert_eq!(parse("\"a\" { \"b\" \"1\"").unwrap_err(), VdfError::UnclosedBlock);
        assert_eq!(parse("\"a\" }").unwrap_err(), VdfError::KeyWithoutValue("a".into()));
        assert_eq!(parse("}").unwrap_err(), VdfError::UnexpectedClose);
        assert_eq!(parse("{ }").unwrap_err(), VdfError::BlockWithoutKey);
        assert_eq!(parse("\"a").unwrap_err(), VdfError::UnclosedQuote);
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        assert!(parse("\u{feff}\"a\" \"b\"").unwrap().text("a").is_some());
    }
}
