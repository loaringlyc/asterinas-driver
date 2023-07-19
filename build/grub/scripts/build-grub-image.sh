#! /bin/sh

# Cargo passes the path to the built executable as the first argument.
KERNEL=$1

# Copy the needed files into an ISO image.
mkdir -p target/iso_root
cp $KERNEL target/iso_root/jinux
mkdir -p target/iso_root/boot/grub
cp build/grub/conf/grub.cfg target/iso_root/boot/grub

# Copy ramdisk
cp regression/build/ramdisk.cpio.gz target/iso_root

# Make boot device .iso image
grub-mkrescue -o $KERNEL.iso target/iso_root
