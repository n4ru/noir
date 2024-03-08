VERSION 0.8
IMPORT github.com/earthly/lib/rust:2.2.11 AS rust

FROM node:18.19.1

# Install the version of Rust as described in `rust-toolchain.toml`
COPY ./rust-toolchain.toml .
ENV RUST_VERSION=$(grep '^channel =' ./rust-toolchain.toml | sed -E 's/channel = "([^"]+)"/\1/')

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH

RUN set -eux; \
    dpkgArch="$(dpkg --print-architecture)"; \
    case "${dpkgArch##*-}" in \
        amd64) rustArch='x86_64-unknown-linux-gnu'; rustupSha256='0b2f6c8f85a3d02fde2efc0ced4657869d73fccfce59defb4e8d29233116e6db' ;; \
        armhf) rustArch='armv7-unknown-linux-gnueabihf'; rustupSha256='f21c44b01678c645d8fbba1e55e4180a01ac5af2d38bcbd14aa665e0d96ed69a' ;; \
        arm64) rustArch='aarch64-unknown-linux-gnu'; rustupSha256='673e336c81c65e6b16dcdede33f4cc9ed0f08bde1dbe7a935f113605292dc800' ;; \
        i386) rustArch='i686-unknown-linux-gnu'; rustupSha256='e7b0f47557c1afcd86939b118cbcf7fb95a5d1d917bdd355157b63ca00fc4333' ;; \
        *) echo >&2 "unsupported architecture: ${dpkgArch}"; exit 1 ;; \
    esac; \
    url="https://static.rust-lang.org/rustup/archive/1.26.0/${rustArch}/rustup-init"; \
    wget "$url"; \
    echo "${rustupSha256} *rustup-init" | sha256sum -c -; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --profile minimal --default-toolchain $RUST_VERSION --default-host ${rustArch}; \
    rm rustup-init; \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME; \
    rustup --version; \
    cargo --version; \
    rustc --version;


# Install various tools used for building JS packages
RUN apt-get update && apt-get install --no-install-recommends -qq jq
COPY --dir ./.github/scripts .
RUN ./scripts/wasm-bindgen-install.sh
RUN ./scripts/wasm-opt-install.sh

source:
    # TODO: we're pulling in a lot of non-rust source here, e.g. READMEs. 
    WORKDIR ./project
    COPY Cargo.toml Cargo.lock rust-toolchain.toml .rustfmt.toml ./
    # TODO: yikes but necessary for injecting the git commit. Can we push that off until a later step?
    COPY --dir .git ./.git
    COPY --dir acvm-repo/acir acvm-repo/acir_field acvm-repo/acvm acvm-repo/acvm_js acvm-repo/blackbox_solver acvm-repo/bn254_blackbox_solver acvm-repo/brillig acvm-repo/brillig_vm ./acvm-repo
    COPY --dir compiler/fm compiler/noirc_driver compiler/noirc_errors compiler/noirc_evaluator compiler/noirc_frontend compiler/noirc_printable_type compiler/utils compiler/wasm ./compiler
    COPY --dir tooling/backend_interface tooling/bb_abstraction_leaks tooling/debugger tooling/lsp tooling/nargo tooling/nargo_cli tooling/nargo_fmt tooling/nargo_toml tooling/noirc_abi tooling/noirc_abi_wasm ./tooling
    COPY --dir aztec_macros noir_stdlib test_programs ./

    SAVE ARTIFACT ./*


build:
    FROM +source
    RUN cargo build --release
    SAVE ARTIFACT ./target/release/nargo

fmt:
  FROM +source
  RUN cargo fmt --check --all

clippy:
  FROM +source
  RUN cargo clippy --workspace

# Pull in `package.json`s for yarn workspace and install all dependencies for caching
yarn-deps:
    WORKDIR ./project
    COPY --dir .yarn ./
    COPY .yarnrc.yml ./

    COPY package.json ./
    COPY acvm-repo/acvm_js/package.json ./acvm-repo/acvm_js/package.json
    COPY compiler/wasm/package.json ./compiler/wasm/package.json
    COPY compiler/integration-tests/package.json ./compiler/integration-tests/package.json
    COPY tooling/noir_codegen/package.json ./tooling/noir_codegen/package.json
    COPY tooling/noir_js/package.json ./tooling/noir_js/package.json
    COPY tooling/noir_js_backend_barretenberg/package.json ./tooling/noir_js_backend_barretenberg/package.json
    COPY tooling/noir_js_types/package.json ./tooling/noir_js_types/package.json
    COPY tooling/noirc_abi_wasm/package.json ./tooling/noirc_abi_wasm/package.json
    COPY docs/package.json ./docs/package.json
    COPY yarn.lock ./

    RUN yarn install --immutable

yarn-source:
    FROM +yarn-deps
    COPY --dir tooling/noir_codegen tooling/noir_js tooling/noir_js_backend_barretenberg tooling/noir_js_types ./tooling
    COPY --dir ./docs ./
    
yarn-build:
    # TODO: build wasm bundles in parallel
    # TODO: move docs builds off of the critical path
    FROM +yarn-source
    COPY --dir +source/* .
    COPY --dir ./docs .
    
    RUN yarn build
