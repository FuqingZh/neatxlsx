ARG BASE_IMAGE
FROM ${BASE_IMAGE}

ARG RUST_TOOLCHAIN=1.96.1
ENV CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup \
    PATH=/root/.cargo/bin:${PATH}

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain ${RUST_TOOLCHAIN} && \
    rustc --version && cargo --version

