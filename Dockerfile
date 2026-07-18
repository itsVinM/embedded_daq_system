FROM rustembedded/cross:thumbv7em-none-eabihf

RUN apt-get update && apt-get install -y \
    pkg-config \
    libusb-1.0-0-dev \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install probe-rs --features=cli

WORKDIR /workspace
