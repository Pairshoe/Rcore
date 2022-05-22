use easy_fs::{
    EasyFileSystem,
    Inode,
};
use crate::drivers::BLOCK_DEVICE;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use lazy_static::*;
use bitflags::*;
use alloc::vec::Vec;
use crate::fs::{StatMode};
use super::File;
use crate::mm::UserBuffer;

/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}

/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

impl OSInode {
    /// Construct an OS inode from a inode
    pub fn new(
        readable: bool,
        writable: bool,
        inode: Arc<Inode>,
    ) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner {
                offset: 0,
                inode,
            })},
        }
    }
    /// Read all data inside a inode into vector
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}

lazy_static! {
    /// The root of all inodes, or '/' in short
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

pub fn get_nlink(target_block_id: u32, target_block_offset: usize) -> u32 {
    ROOT_INODE.get_nlink(target_block_id, target_block_offset)
}

/// List all files in the filesystems
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    /// Flags for opening files
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Get the current read write permission on an inode
    /// does not check validity for simplicity
    /// returns (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open a file by path
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            // clear size
            inode.clear();
            Some(Arc::new(OSInode::new(
                readable,
                writable,
                inode,
            )))
        } else {
            // create file
            ROOT_INODE.create(name)
                .map(|inode| {
                    Arc::new(OSInode::new(
                        readable,
                        writable,
                        inode,
                    ))
                })
        }
    } else {
        ROOT_INODE.find(name)
            .map(|inode| {
                if flags.contains(OpenFlags::TRUNC) {
                    inode.clear();
                }
                Arc::new(OSInode::new(
                    readable,
                    writable,
                    inode
                ))
            })
    }
}

pub fn link_file(old_name: &str, new_name: &str) -> isize {
    if let Some(mut old_inode) = ROOT_INODE.find(old_name) {
        let old_ino = old_inode.get_ino() as u32;
        return ROOT_INODE.link(old_ino, new_name);
    }
    -1
}

pub fn unlink_file(_name: &str) -> isize {
    ROOT_INODE.unlink(_name)
}

impl File for OSInode {
    fn readable(&self) -> bool { self.readable }
    fn writable(&self) -> bool { self.writable }
    fn get_ino(&self) -> u32 {
        let inner = self.inner.exclusive_access();
        inner.inode.get_ino()
    }
    fn get_mode(&self) -> StatMode {
        let inner = self.inner.exclusive_access();
        let mut mode = StatMode::NULL;
        if inner.inode.get_mode() == 0 {
            mode = StatMode::NULL;
        } else if inner.inode.get_mode() == 1 {
            mode = StatMode::DIR;
        } else {
            mode = StatMode::FILE;
        }
        mode
    }
    fn get_nlink(&self, target_block_id: u32, target_block_offset: usize) -> u32 {
        let inner = self.inner.exclusive_access();
        inner.inode.get_nlink(target_block_id, target_block_offset)
    }
    fn get_block_id(&self) -> u32 {
        let inner = self.inner.exclusive_access();
        inner.inode.get_block_id()
    }
    fn get_block_offset(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.inode.get_block_offset()
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}
