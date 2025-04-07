# SPDX-FileCopyrightText: 2019-2025 Michael Picht <mipi@fsfe.org>
#
# SPDX-License-Identifier: GPL-3.0-or-later

################################################################################
# This is a template Dockerfile. Replace '{{BASE_IMAGE}}' by a real image name #
# before using                                                                 #
################################################################################

## Builder stage
FROM {{BASE_IMAGE}} as builder
# Create and set up build directory
RUN mkdir /build
COPY . /build/
WORKDIR /build
# Set/modify environment for rust
ENV PATH="/root/.cargo/bin:${PATH}"
ENV RUSTUP_HOME="/root/.rustup"
# 1. Disable space check in pacman. This is necessary since the free disk space
#    might not be reported properly inside the container. In that case, pacman
#    could stop with an error (see https://github.com/docker/for-mac/issues/5433)
# 2. Disable sandbox (see https://bbs.archlinux.org/viewtopic.php?id=299402) 
# 3. Install required software (clang is only required because of this
#    workaround: https://github.com/archlinux/alpm.rs/issues/35) and clean
#    package cache afterwards
# 4. Set up rust
# 5. Build repman
RUN sed -i 's/CheckSpace/#CheckSpace/g;s/#DisableSandbox/DisableSandbox/g' /etc/pacman.conf && \
    pacman --noconfirm -Syu asciidoctor clang gcc git make pkg-config rustup && \
    pacman --noconfirm -Scc && \
    rustup install --profile minimal stable && \
    make

## Runtime stage
FROM {{BASE_IMAGE}} as runtime
WORKDIR /
# Copy repman artifacts from builder stage to target location
ARG PROG=repman
COPY --from=builder /build/target/release/${PROG} /usr/bin/${PROG}
COPY --from=builder /build/${PROG}-all /usr/bin/${PROG}-all
COPY --from=builder /build/${PROG}.8 /usr/share/man/man8/${PROG}.8
COPY --from=builder /build/cfg/${PROG}.conf /etc/${PROG}.conf
# 1. Switch off debug option in makepkg.conf since this leads to building
#    ...-debug packages (which is not wanted)
# 2. Disable space check in pacman. This is necessary since the free disk space
#    might not be reported properly inside the container. In that case, pacman
#    could stop with an error (see https://github.com/docker/for-mac/issues/5433)
# 3. Disable sandbox (see https://bbs.archlinux.org/viewtopic.php?id=299402) 
# 4. Install dependencies and clean package cache afterwards
# 5. Configure sudo (sudo is required by the scripts that repman is calling)
# 6. Create user to execute repman (repman cannot be executed as root since it
#    ultimately calls makepkg which cannot be executed as root)
RUN sed -i 's/ debug/ \!debug/g;s/(debug/(\!debug/g' /etc/makepkg.conf && \
    sed -i 's/CheckSpace/#CheckSpace/g;s/#DisableSandbox/DisableSandbox/g' /etc/pacman.conf && \
    pacman --noconfirm -Syu base-devel  && \
    pacman --noconfirm -Scc && \
    echo '%wheel ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    useradd -mU -G sys,wheel ${PROG}_user
WORKDIR /
CMD ["repman", "-V"]
