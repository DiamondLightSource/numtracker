FROM rust:1.87.0-slim AS build

RUN rustup target add x86_64-unknown-linux-musl && \
    apt-get update && \
    apt-get install -y musl-tools musl-dev && \
    update-ca-certificates

WORKDIR /build

# Jump through hoops here to improve the caching performance.
# As each command creates a new layer that can be re-used as long as nothing
# has changed, maximise the amount of work done before the frequently updated
# files (.git and src) are included

# Create an empty project
RUN cargo init --name empty-build-init

# Copy the cargo config but no source
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

# Build the empty project with all the dependencies
RUN cargo build --release --target x86_64-unknown-linux-musl

# Copy the rest of the project in and build again
COPY ./build.rs ./build.rs
COPY ./.env ./.env
COPY ./.sqlx ./.sqlx
COPY ./migrations ./migrations
COPY ./queries ./queries
COPY ./static ./static
COPY ./src ./src
# Copy the git directory purely so that the commit information can be included
# build details provided by the 'built' library. See #99
COPY ./.git ./.git

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch

LABEL org.opencontainers.image.source=https://github.com/DiamondLightSource/numtracker.git
LABEL org.opencontainers.image.description="Central co-ordinator for scan numbers and file locations"
LABEL org.opencontainers.image.licenses=Apache-2.0

COPY --from=build /build/target/x86_64-unknown-linux-musl/release/numtracker /app/numtracker

CMD ["serve"]
ENTRYPOINT ["/app/numtracker"]
