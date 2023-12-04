## Builder stage
FROM mkaczanowski/archlinuxarm as builder
# Create an set up build directory
RUN mkdir /build
COPY . /build/
WORKDIR /build
# Install required software and clean package cache afterwards
RUN pacman --noconfirm -Syu asciidoctor gcc git make pkg-config rustup
RUN pacman --noconfirm -Scc
# Set up rust
RUN rustup install --profile minimal stable
ENV PATH="/root/.cargo/bin:${PATH}"
ENV RUSTUP_HOME="/root/.rustup"
# Build repman
RUN make

## Runtime stage
FROM mkaczanowski/archlinuxarm as runtime
WORKDIR /
# Copy repman artefacts from builder stage to target location
ARG PROG=repman
COPY --from=builder /build/target/release/${PROG} /usr/bin/${PROG}
COPY --from=builder /build/${PROG}-all /usr/bin/${PROG}-all
COPY --from=builder /build/${PROG}.8 /usr/share/man/man8/${PROG}.8
COPY --from=builder /build/cfg/${PROG}.conf /etc/${PROG}.conf
# Install dependencies
RUN pacman --noconfirm -Syu fakeroot binutils sudo
# Configure sudo (sudo is required by the scripts that repman is calling)
RUN echo '%wheel ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers
# Create user to execute repman (repman cannot be executed as root since it
# ultimately calls makepkg which cannot be executed as root) 
RUN useradd -mU -G sys,wheel ${PROG}_user
WORKDIR /
CMD ["repman", "-V"]
