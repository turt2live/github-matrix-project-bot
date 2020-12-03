FROM rust:1.48
WORKDIR /usr/src/myapp
COPY . .
RUN cargo install --path .
CMD ["/usr/local/cargo/bin/github-matrix-project"]
