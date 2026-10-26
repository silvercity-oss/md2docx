# md2docx with the in-process Mermaid renderer, as a small unprivileged image.
# Build:  docker build -t silvercity/md2docx:local .
# Run:    docker run --rm -i --network none --read-only --init -e MD2DOCX_UNTRUSTED=1 \
#           silvercity/md2docx:local - < in.md > out.docx
# See docs/containers.md.
ARG RUST_VERSION=1.95

FROM rust:${RUST_VERSION}-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --features mermaid --bin md2docx

# Same Debian release as the builder, so the glibc matches.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/md2docx /md2docx
USER nonroot
ENTRYPOINT ["/md2docx"]
