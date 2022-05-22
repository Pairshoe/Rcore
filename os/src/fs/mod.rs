mod stdio;
mod inode;

use crate::mm::UserBuffer;

/// The common abstraction of all IO resources
pub trait File : Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn get_ino(&self) -> u32;
    fn get_mode(&self) -> StatMode;
    fn get_nlink(&self, target_block_id: u32, target_block_offset: usize) -> u32;
    fn get_block_id(&self) -> u32;
    fn get_block_offset(&self) -> usize;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
}

/// The stat of a inode
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    pub dev: u64,
    /// inode number
    pub ino: u64,
    /// file type and mode
    pub mode: StatMode,
    /// number of hard links
    pub nlink: u32,
    /// unused pad
    pub pad: [u64; 7],
}

bitflags! {
    /// The mode of a inode
    /// whether a directory or a file
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// directory
        const DIR   = 0o040000;
        /// ordinary regular file
        const FILE  = 0o100000;
    }
}

pub use stdio::{Stdin, Stdout};
pub use inode::{OSInode, open_file, link_file, unlink_file, get_nlink, OpenFlags, list_apps};
