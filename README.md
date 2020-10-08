# rust-vfs

[![Crate](https://img.shields.io/crates/v/vfs.svg)](https://crates.io/crates/vfs)
[![API](https://docs.rs/vfs/badge.svg)](https://docs.rs/vfs)
![Minimum rustc version](https://img.shields.io/badge/rustc-1.32.0+-green.svg)
[![Actions Status](https://github.com/manuel-woelker/rust-vfs/workflows/Continuous%20integration/badge.svg)](https://github.com/manuel-woelker/rust-vfs/actions?query=workflow%3A%22Continuous+integration%22)
[![Build Status](https://travis-ci.org/manuel-woelker/rust-vfs.svg?branch=master)](https://travis-ci.org/manuel-woelker/rust-vfs)

A virtual filesystem for Rust

The virtual file system abstraction generalizes over file systems and allows using
different filesystem implementations (e.g. an in memory implementation for unit tests)

This crate currently has the following implementations:
 * **PhysicalFS** - the actual filesystem of the underlying OS
 * **MemoryFS** - an ephemeral in-memory file system, intended mainly for unit tests
 * **AltrootFS** - a file system with its root in a particular directory of another filesystem
 * **OverlayFS** - an overlay file system combining two filesystems, an upper layer with read/write access and a lower layer with only read access
 * **EmbeddedFS** - a read-only file system embedded in the executable, requires `embedded-fs` feature
 
The minimum supported Rust version is 1.32.0.
 
Comments and pull-requests welcome!

## Changelog

### 0.4.0 (2020-08-13)

 * Added `OverlayFS` union filesystem
 * Added `VfsPath::read_to_string()` convenience method
 * Added `VfsPath::walk_dir()` method for recursive directory traversal
 * Added `VfsPath::{copy,move}_{file,dir}()` methods (closes [#9](https://github.com/manuel-woelker/rust-vfs/issues/9))
 * License is now Apache 2.0
 * Minimum supported Rust version (MSRV) is 1.32.0

### 0.3.0 (2020-08-04)

 * Refactored to use a trait based design, simplifying usage and testing
 
### 0.2.1 (2020-02-06)

 * Added `AltrootFS` (thanks [@icefoxen](https://github.com/icefoxen))

### 0.1.0 (2016-05-14)

 * Initial release
 
## Roadmap

 * Support for read-only filesystems  
 * Support for re-mounting filesystems
 * Support for virtual filesystem access inside archives (e.g. zip)
