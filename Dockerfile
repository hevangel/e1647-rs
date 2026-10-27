# Toolchain container for e1647-rs: everything needed to build and test the
# crate with every cargo feature, including the Pumpkin solver backend
# (whose build script requires a C compiler — the reason this container
# exists; the local windows-gnu toolchain has none).
#
# Credentials never enter this image: `.env` is excluded from the build
# context via .dockerignore and must not be copied in.

FROM rust:1-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        gcc \
        g++ \
        libc6-dev \
        pkg-config \
        git \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

# Compiled artifacts live outside /workspace so the warm dependency cache
# baked into this image survives the repository bind mount at run time.
ENV CARGO_TARGET_DIR=/opt/target

# Prefetch the full dependency graph (all features) at image-build time so
# runs only compile, not download. The crate itself is deliberately NOT
# compiled here: bind-mount mtime skew can make cargo reuse stale artifacts
# of these placeholders instead of the mounted sources. The placeholder
# sources are removed — the real repository is bind-mounted over /workspace.
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo "" > src/lib.rs \
    && cargo fetch --locked \
    && rm -rf src

CMD ["bash"]
