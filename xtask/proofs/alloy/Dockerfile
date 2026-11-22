# The offline host of the Alloy obligations, which `just proofs` runs with `--network none`.
FROM docker.io/library/eclipse-temurin:26-jre@sha256:0dbf52b3861a84e2a19c503245504e5a71a5f071ccb9c48997efd4a3454a52b7

# The release publishes no checksum file, so this file pins the digest of the jar the release serves.
ARG ALLOY_VERSION=6.2.0
ARG ALLOY_SHA256=6b8c1cb5bc93bedfc7c61435c4e1ab6e688a242dc702a394628d9a9801edb78d
RUN mkdir -m 0755 /opt/alloy
ADD --checksum=sha256:${ALLOY_SHA256} https://github.com/AlloyTools/org.alloytools.alloy/releases/download/v${ALLOY_VERSION}/org.alloytools.alloy.dist.jar /opt/alloy/alloy.jar
RUN echo "${ALLOY_SHA256}  /opt/alloy/alloy.jar" | sha256sum -c - \
    && chmod 0444 /opt/alloy/alloy.jar
