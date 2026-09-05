# syntax=docker/dockerfile:1.7
FROM rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 AS build
WORKDIR /src
COPY . .
RUN cargo build --locked --release --bin inferqos-site

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
COPY --from=build /src/target/release/inferqos-site /usr/local/bin/inferqos-site
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/inferqos-site"]
