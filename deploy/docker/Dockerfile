# syntax=docker/dockerfile:1.7
FROM rust:1.98.1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --locked --release --bin inferqos

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/inferqos /usr/local/bin/inferqos
USER nonroot:nonroot
EXPOSE 8080 9090
ENTRYPOINT ["/usr/local/bin/inferqos"]
CMD ["serve", "--config", "/etc/inferqos/inferqos.yaml"]
