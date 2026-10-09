# The offline host of the Alloy obligations, which `just proofs` runs with `--network none`.
FROM docker.io/library/eclipse-temurin:21-jre@sha256:cff19e6215689161eb6162c11b86b0c60ddf802164f2eaf48d570f8fb79a36c5

# The release publishes no checksum file, so this file pins the digest of the jar the release serves.
ARG ALLOY_VERSION=6.2.0
ARG ALLOY_SHA256=6b8c1cb5bc93bedfc7c61435c4e1ab6e688a242dc702a394628d9a9801edb78d
ADD --checksum=sha256:${ALLOY_SHA256} https://github.com/AlloyTools/org.alloytools.alloy/releases/download/v${ALLOY_VERSION}/org.alloytools.alloy.dist.jar /opt/alloy/alloy.jar
