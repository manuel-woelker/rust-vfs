# rust-vfs

A virtual filesystem for Rust

The virtual file system abstraction generalizes over file systems and allow using
different filesystem implementations (i.e. an in memory implementation for unit tests)

A virtual filesystem consists of three basic types

 * **Paths** - locations in the filesystem
 * **File** - actual file contents (think inodes)
 * **Metadata** - metadata information about paths

This crate currently has the following implementations:
 * **PhysicalFS** - the actual filesystem of the underlying OS
 * **MemoryFS** - an ephemeral in-memory implementation (intended for unit tests)
