# Implementing Virtio-gpu for Asterinas

## Getting Started

Get yourself an x86-64 Linux machine with Docker installed.
Follow the three simple steps below to get Asterinas up and running.

### 1. Download the latest source code.

```bash
git clone https://github.com/loaringlyc/asterinas-driver.git
```

### 2. Run a Docker container as the development environment.

The official method goes as follows:

```bash
docker run -it --privileged --network=host --device=/dev/kvm -v $(pwd)/asterinas:/root/asterinas asterinas/asterinas:0.11.0
```

However, using this command may lead to some network problems. I highly recommend you to download the `asterinas-docker.tar.xz` file, unpack it into `asterinas-docker.tar` file, and then run the following command:

```bash
sudo podman load < /path/to/your/file/asterinas-docker.tar
sudo podman run -it --privileged --network=host --device=/dev/kvm -v $(pwd)/asterinas-driver:/root/asterinas docker.io/asterinas/asterinas:0.9.4
# note that sudo is necessary for podman for --device=/dev/kvm arg
``` 

or if you use docker:
```bash
docker load < /path/to/your/file/asterinas-docker.tar
docker run -it --privileged --network=host --device=/dev/kvm -v $(pwd)/asterinas-driver:/root/asterinas docker.io/asterinas/asterinas:0.9.4
```

### 3. Inside the container, go to the project folder to build and run Asterinas.

```bash
make build
make run
```

If everything goes well, Asterinas is now up and running inside a VM.

## Git 

Here are some git instructions:

```bash
git remote -v
git commit -m "your informtion"
git push -u origin main

git pull origin main:main
```

## The Book

See [The Asterinas Book](https://asterinas.github.io/book/) to learn more about the project.
