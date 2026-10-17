# Construction de l'AppImage de Last1 Panel, comme celle de SoundFence : la
# même sur GitHub Actions et sur un PC (packaging/appimage/build.sh).
#
# Une AppImage tourne sur les systèmes au moins aussi récents que celui où elle
# est construite : Debian 12 (glibc 2.36) couvre Arch, CachyOS et toutes les
# distributions de jeu actuelles.
FROM debian:12

ARG RUST_VERSION=1.97.1
ARG BUN_VERSION=1.4.2

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential ca-certificates curl file librsvg2-dev libssl-dev \
        libwebkit2gtk-4.1-dev pkg-config rsync unzip \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    BUN_INSTALL=/opt/bun \
    PATH=/opt/cargo/bin:/opt/bun/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION" \
    && curl -fsSL https://bun.sh/install | bash -s "bun-v$BUN_VERSION"

# Les outils AppImage du bundler sont eux-mêmes des AppImage : pas de FUSE
# dans un conteneur, ils s'extraient d'eux-mêmes.
ENV APPIMAGE_EXTRACT_AND_RUN=1

COPY build.sh /usr/local/bin/build-last1-panel
ENTRYPOINT ["build-last1-panel"]
