# Linux verification host for `xtask proofs` where Kani has no native build.
FROM docker.io/library/rust:1.99.1-bookworm@sha256:e12566d5abc17a0da741628782cdb504b68b8961f5bfcf677936bf3e8b32de29

RUN cargo install --locked kani-verifier --version 0.68.0 \
    && cargo kani setup \
    && cargo install --locked cargo-mutants --version 27.1.0 \
    && cargo install --locked cargo-fuzz --version 0.13.2 \
    && rm -rf /usr/local/cargo/registry

# The gungraun benchmarks of the budget gate count instructions and heap peaks with Valgrind, which no other host of this repository runs.
RUN apt-get update \
    && apt-get install -y --no-install-recommends valgrind \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install --locked gungraun-runner --version 0.20.0 \
    && rm -rf /usr/local/cargo/registry

# Miri interprets the pure core's tests for the host and for Windows on the nightly that Kani 0.68.0 pins.
# Each run builds its sysroots, because a sysroot built here doesn't match the settings of a run.
RUN rustup toolchain install nightly-2026-08-21 --profile minimal --component miri,rust-src

ARG POWERSHELL_VERSION
ARG POWERSHELL_SHA256
ARG ANALYZER_VERSION
ARG ANALYZER_SHA256
ARG MISE_VERSION
ARG MISE_SHA256

RUN curl -fsSL "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-linux-x64" -o /usr/local/bin/mise \
    && echo "${MISE_SHA256}  /usr/local/bin/mise" | sha256sum -c - \
    && chmod +x /usr/local/bin/mise

# The image saves the analyzer release that the native gate pins.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libicu72 zsh \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /opt/powershell \
    && curl -fsSL "https://github.com/PowerShell/PowerShell/releases/download/v${POWERSHELL_VERSION}/powershell-${POWERSHELL_VERSION}-linux-x64.tar.gz" -o /tmp/powershell.tar.gz \
    && echo "${POWERSHELL_SHA256}  /tmp/powershell.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/powershell.tar.gz -C /opt/powershell \
    && chmod +x /opt/powershell/pwsh \
    && ln -s /opt/powershell/pwsh /usr/local/bin/pwsh \
    && pwsh -NoLogo -NoProfile -NonInteractive -Command '$ErrorActionPreference = "Stop"; Save-Module -Name PSScriptAnalyzer -RequiredVersion $env:ANALYZER_VERSION -Path /root/.cache/dotfiles/psscriptanalyzer -Repository PSGallery -Force; $assembly = "/root/.cache/dotfiles/psscriptanalyzer/PSScriptAnalyzer/$env:ANALYZER_VERSION/PSv7/Microsoft.Windows.PowerShell.ScriptAnalyzer.dll"; if ((Get-FileHash -LiteralPath $assembly -Algorithm SHA256).Hash.ToLowerInvariant() -ne $env:ANALYZER_SHA256) { throw "Analyzer checksum mismatch" }' \
    && rm /tmp/powershell.tar.gz
