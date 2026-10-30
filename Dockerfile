# --- Xezim SystemVerilog simulator -----------------------------------
# Built from the pinned upstream rev recorded in the ecosystem toolchain
# image (ai_hw_engineer). Mixed e + SystemVerilog testcases drive it
# through the VPI bridge compiled into the toolchain stage below.
FROM rust:1-slim AS xezim-build
ARG XEZIM_REV=058f2bba58648452e5799d17572356ebcae7d163
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates git build-essential pkg-config \
        autoconf automake libtool \
    && rm -rf /var/lib/apt/lists/* \
    && git clone --filter=blob:none https://github.com/aionhw/xezim.git /opt/xezim-src \
    && git -C /opt/xezim-src checkout --detach "${XEZIM_REV}" \
    && CARGO_TARGET_DIR=/opt/xezim-target cargo build --release --bin xezim --manifest-path /opt/xezim-src/Cargo.toml \
    && mkdir -p /opt/xezim/bin \
    && cp /opt/xezim-target/release/xezim /opt/xezim/bin/xezim

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

# Xezim on PATH plus the VPI control bridge for the hdl_* builtins.
COPY --from=xezim-build /opt/xezim /opt/xezim
COPY --from=xezim-build /opt/xezim-src/include /opt/xezim-src/include
COPY c/xezim_bridge.c /opt/xezim/src/xezim_bridge.c
RUN cc -shared -fPIC -I/opt/xezim-src/include \
        -o /opt/xezim/bridge.so /opt/xezim/src/xezim_bridge.c \
    && rm /opt/xezim/src/xezim_bridge.c
ENV PATH="/opt/xezim/bin:${PATH}" \
    E1647_XEZIM_BRIDGE=/opt/xezim/bridge.so

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
