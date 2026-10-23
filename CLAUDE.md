# Last1 Panel — consignes pour Claude Code

L'utilisateur veut des actions, pas des analyses : réponses courtes, en français.
Les joueurs ne choisissent rien de technique : Last1 Panel décide et applique ce
qui est le mieux ; ce qui n'est pas sûr d'aider n'est ni activé ni proposé.

## Notes de mise à jour

Chaque commit qui change ce que voit ou vit un joueur porte, dans le bloc de fin
du message (avec Co-Authored-By), une phrase simple pour lui, en français et en
anglais :

```
Joueurs: La carte graphique se libère d'un clic dans Performances.
Players: The graphics card unlocks in one click in Performance.
```

Clés sans accent (git ne lit pas « Nouveauté: » comme ligne de fin). Plusieurs
lignes `Joueurs:` / `Players:` si le commit apporte plusieurs choses. Rien de
visible pour les joueurs (outils, CI, tests, refactorisation) : pas de ligne.

La CI en fait la liste (`tools/release-notes.sh` → `release-notes.tsv`,
embarqué par `core/build.rs`) ; après une mise à jour, le service de la session
en montre trois dans la notification « à jour », et la fenêtre les montre toutes
jusqu'à ce que le joueur les ferme (`core/src/release_notes.rs`). Même format
que go-to-linux.

## Vérifications avant de pousser sur main

```bash
cargo fmt --all --check
cargo +stable clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bun run check
```

Chaque poussée sur `main` publie une version sur last1.fun (CI GitHub).
