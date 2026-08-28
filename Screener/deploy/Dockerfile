# Uses the prebuilt static binary in deploy/bin/ so the image builds in
# seconds even on a 1 GB VM. Build context is the workspace root:
#     docker build -f deploy/Dockerfile -t screener-service .
# (docker compose in this directory already sets `context: ..`).
#
# deploy/bin/screener-service is a fully static x86_64 musl build — no glibc,
# runs on any x86_64 Linux. Rebuild it with deploy/build-binary.sh after
# changing screener-service / screener-core source, then rebuild this image.
#
# To compile inside Docker instead (slow), use deploy/Dockerfile.fromsource.

FROM alpine:3
RUN apk add --no-cache ca-certificates
COPY deploy/bin/screener-service /usr/local/bin/screener-service
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/screener-service"]
