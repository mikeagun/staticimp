#builder image
FROM rust:1.71-alpine3.18 as builder
MAINTAINER Michael Agun <mikeagun@gmail.com>

RUN apk add --no-cache musl-dev openssl-dev

# create dummy project to build our dependencies into a cache
RUN cargo new /usr/src/staticimp
WORKDIR /usr/src/staticimp

COPY Cargo.toml ./

# build empty project with all our dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release

# now we actually copy the project over and build/install
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
  touch src/main.rs \
  && cargo install --path .



#staticimp image
FROM alpine:3.18
MAINTAINER Michael Agun <mikeagun@gmail.com>

ARG USER=staticimp

# add new user
RUN adduser -D $USER
USER $USER
ENV HOME /home/$USER

WORKDIR /


COPY --from=builder --chown=$USER --chmod=500 /usr/local/cargo/bin/staticimp /usr/local/bin/staticimp
COPY --chown=$USER --chmod=400 ./staticimp.sample.yml /staticimp.yml
COPY --chown=$USER --chmod=500 ./healthcheck.sh /healthcheck.sh

HEALTHCHECK --interval=30s --timeout=3s --start-period=3s --retries=2 CMD ["/healthcheck.sh"]

EXPOSE 8080

CMD ["staticimp"]
