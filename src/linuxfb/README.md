# Rust interface to the Linux Framebuffer API

[![Crate version](https://img.shields.io/crates/v/linuxfb?style=flat-square)](https://crates.io/crates/linuxfb/)

![Crate license](https://img.shields.io/crates/l/linuxfb?style=flat-square)

[![Build](https://img.shields.io/gitlab/pipeline/nilclass/rust-linuxfb?style=flat-square)](https://gitlab.com/nilclass/rust-linuxfb/pipelines)

Provides a safe, rusty wrapper around the Linux Framebuffer API (`linux/fb.h`).

[Online Documentation](https://docs.rs/linuxfb/latest)

## Features & Scope

* Discover & open framebuffer devices
* Read useful information from the device, such as:
  * Size of the display, in pixels and millimeters
  * Pixel-level layout (color channels, bytes per pixel)
  * Virtual display size, to use for panning, double-buffering, etc.
* Modify virtual size, panning offset and bytes-per-pixel (for some drivers this allows switching between 32-bit and 16-bit mode)
* Set blanking mode (turns the screen on and off)
* Map the device into memory (provides you with a `&mut [u8]` slice to write to)
* Optional wrapper providing a double-buffered surface, that can be "flipped"

This package does **NOT** deal with:
* Drawing of any kind (you get a buffer, it's up to you to fill it)
* Color representation and conversion

## Getting started

First of all, make sure you have access to your framebuffer device.
You can usually do that by running as root (definitely *not* recommended), or by adding yourself to the `video` group.

There are two examples provided in the documentation:
1. Accessing, configuring and using the framebuffer in the [`linuxfb::Framebuffer documentation`](https://docs.rs/linuxfb/latest/linuxfb/struct.Framebuffer.html)
2. Using the optional double-buffering implementation in the [`linuxfb::double::Buffer documentation`](https://docs.rs/linuxfb/latest/linuxfb/double/struct.Buffer.html)

## Contributing

- The upstream source can be found [here](https://gitlab.com/nilclass/rust-linuxfb)
- Pull requests are generally accepted, if they fit the scope of the package
