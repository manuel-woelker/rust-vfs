# rust-vfs

[![Crate](https://img.shields.io/crates/v/vfs.svg)](https://crates.io/crates/vfs)
[![API](https://docs.rs/vfs/badge.svg)](https://docs.rs/vfs)
![Minimum rustc version](https://img.shields.io/badge/rustc-1.61.0+-green.svg)
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
 * **EmbeddedFS** - a read-only file system embedded in the executable, requires `embedded-fs` feature, no async version available
 
The minimum supported Rust version is 1.61.0.
 
Comments and pull-requests welcome!

## Changelog

### 0.10.0 (2023-09-08)
* Added async port of the crate, in a new module `async_vfs`.
The module is behind the `async-vfs` feature flag which is not enabled by default.
* Ported all synchronous tests and doc-tests to async
* Updated minimum supported Rust version to 1.61.0, needed for the async port.
* Updated Rust edition from *2018* to *2021*, needed for the async port.
* Updated Rust versions used in CI pipeline.

### 0.9.0 (2022-12-20)

* prevent `Path::create_dir_all()` failures when executing in parallel
  (fixes [#47](https://github.com/manuel-woelker/rust-vfs/pull/47))
* Allow absolute paths (e.g. starting with "/") in `VfsPath::join()`
  ([#45](https://github.com/manuel-woelker/rust-vfs/pull/45) - thanks [@Property404](https://github.com/Property404))
* Allow multiple consecutive slashes in paths
  ([#43](https://github.com/manuel-woelker/rust-vfs/pull/43) - thanks [@Property404](https://github.com/Property404))
* Add method `VfsPath::is_root()`
  ([#44](https://github.com/manuel-woelker/rust-vfs/pull/44) - thanks [@Property404](https://github.com/Property404))
* `Path::join()` now allows resolving '..' at the root (resolving to root itself)
 ([#41](https://github.com/manuel-woelker/rust-vfs/pull/41) - thanks [@Property404](https://github.com/Property404))  
*  Add `Send` to trait objects returned from APIs
   ([#40](https://github.com/manuel-woelker/rust-vfs/pull/40),
   [#46](https://github.com/manuel-woelker/rust-vfs/pull/46) - thanks [@Property404](https://github.com/Property404))

### 0.8.0 (2022-11-24)

* Impl `std::error::Error` for `VfsError` ([#32](https://github.com/manuel-woelker/rust-vfs/pull/32)) and improved error 
  ergonomics for end users ([#34](https://github.com/manuel-woelker/rust-vfs/pull/34)) - thanks [@Technohacker](https://github.com/Technohacker)

### 0.7.1 (2022-04-15)

* Fixed a panic when accessing non-existing paths in `MemoryFS::append_file()` (closes 
 [#31](https://github.com/manuel-woelker/rust-vfs/issues/31))

### 0.7.0 (2022-03-26)

* Update to `EmbeddedFS` to `rust-embed` v6 (closes [#29](https://github.com/manuel-woelker/rust-vfs/issues/29))
* Make `OverlayFS` and `AltrootFS` available at the crate root, making it more consistent
  (PR [#30](https://github.com/manuel-woelker/rust-vfs/issues/30) -
  thanks [@Zyian](https://github.com/Zyian))

### 0.6.2 (2022-03-07)

* Activate `embedded-fs` feature when building on docs.rs to ensure that it actually shows up there
  ([#28](https://github.com/manuel-woelker/rust-vfs/issues/28) - thanks [@Absolucy](https://github.com/Absolucy))

### 0.6.1 (2022-03-06)

* Added `VfsPath::root()` method to access the root path of a virtual filesystem
  (closes [#26](https://github.com/manuel-woelker/rust-vfs/issues/26))
* Added doctests to `VfsPath` docs to provide usage examples

### 0.6.0 (2022-03-02)

* Fixed path inconsistency issues in `EmbeddedFS` (closes [#24](https://github.com/manuel-woelker/rust-vfs/issues/24))
* Added the test macro `test_vfs_readonly!` which allows verifying read-only filesystem implementations
* Removed dependency on `thiserror` crate to improve compile times
(closes [#25](https://github.com/manuel-woelker/rust-vfs/issues/25))

### 0.5.2 (2022-02-07)

* Removed potential panic in `OverlayFS` (closes [#23](https://github.com/manuel-woelker/rust-vfs/issues/23))
* `VfsPath::join()` now takes AsRef<str> instead of &str to improve ergonomics with crates like camino

### 0.5.1 (2021-02-13)

* Exported `test_vfs` macro via the feature flag `export-test-macros` to allow downstream implementations to verify 
  expected behaviour
* The MSRV is now 1.40 due to requirements in upstream crates
* The embedded implementation was broken by the 0.5.0 API changes, and is now fixed

### 0.5.0 (2021-02-13)

* Added `EmbeddedFS` for using filesystems embeded in the binary using
[rust-embed](https://github.com/pyros2097/rust-embed)
(PR [#12](https://github.com/manuel-woelker/rust-vfs/issues/12) - thanks [@ahouts](https://github.com/ahouts))
* Changed `VfsPath::exists()` to return `VfsResult<bool>` instead of plain `bool` (closes [#17](https://github.com/manuel-woelker/rust-vfs/issues/17))
 
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
