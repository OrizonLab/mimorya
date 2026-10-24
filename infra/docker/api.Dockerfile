# The API, the operator commands (migrations, companies, invitation links;
# ADR-033), the seed (migrations, fictional demo company) and the process
# that reads deposited documents beside the API (ADR-095), built in release
# mode and run as an unprivileged user (ADR-032).
#   docker build -f infra/docker/api.Dockerfile -t mimorya-api .

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY crates crates
RUN cargo build --release --locked --bin mimorya-api --bin mimorya-admin --bin mimorya-seed \
    --bin mimorya-extract

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home mimorya
COPY --from=build /src/target/release/mimorya-api /src/target/release/mimorya-admin \
    /src/target/release/mimorya-seed /src/target/release/mimorya-extract /usr/local/bin/
USER mimorya
ENV HOST=0.0.0.0 PORT=4000
EXPOSE 4000
CMD ["mimorya-api"]
