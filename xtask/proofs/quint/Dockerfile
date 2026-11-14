# The offline host of the Quint obligations, which `just proofs` runs with `--network none`.
FROM docker.io/library/eclipse-temurin:21-jre@sha256:0b99b3376fbb50dbf9c0af8535fdd5b8eb0d3fdfc8b1e4967f383b6919f6901a AS java

FROM docker.io/library/node:26-bookworm-slim@sha256:5df79989ed685e09f0a98019db4cebfb22fb9df1a53c0161664a011f7ff6b0f1
COPY --from=java /opt/java/openjdk /opt/java/openjdk
ENV PATH=/opt/java/openjdk/bin:/opt/quint/node_modules/.bin:$PATH
ENV QUINT_HOME=/opt/quint

# The lockfile pins Quint and every package it loads by integrity hash.
COPY package.json package-lock.json /opt/quint/
RUN cd /opt/quint && npm ci --ignore-scripts --no-audit --no-fund

# Quint compiles a model through Apalache and runs the model checker that Apalache bundles, with the modules the compiled model extends.
# The release lists the checksum in `sha256sum.txt`, and this file repeats it, so a replaced asset fails the build.
ARG APALACHE_VERSION=0.62.4
ARG APALACHE_SHA256=b781b53bba32fac14ac2e02b0332e1dc2edc64cb7b850c0608d8142cc7aa2ef4
ADD --checksum=sha256:${APALACHE_SHA256} https://github.com/apalache-mc/apalache/releases/download/v${APALACHE_VERSION}/apalache-${APALACHE_VERSION}.tgz /tmp/apalache.tgz
ADD https://github.com/apalache-mc/apalache/releases/download/v${APALACHE_VERSION}/sha256sum.txt /tmp/sha256sum.txt
RUN grep -qx "${APALACHE_SHA256}  apalache-${APALACHE_VERSION}.tgz" /tmp/sha256sum.txt \
    && echo "${APALACHE_SHA256}  /tmp/apalache.tgz" | sha256sum -c - \
    && mkdir -p ${QUINT_HOME}/apalache-dist-${APALACHE_VERSION}/apalache \
    && tar -xzf /tmp/apalache.tgz --strip-components=1 -C ${QUINT_HOME}/apalache-dist-${APALACHE_VERSION}/apalache \
    && rm /tmp/apalache.tgz /tmp/sha256sum.txt \
    && test -f ${QUINT_HOME}/apalache-dist-${APALACHE_VERSION}/apalache/lib/apalache.jar
