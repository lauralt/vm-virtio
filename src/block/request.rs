// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
//
// SPDX-License-Identifier: Apache-2.0 AND BSD-3-Clause

/// Virtio block request.
///
///
use std::{mem, result};

use crate::queue::DescriptorChain;
use vm_memory::{ByteValued, Bytes, GuestAddress, GuestMemory, GuestMemoryError};

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_T_FLUSH: u32 = 4;
const VIRTIO_BLK_T_DISCARD: u32 = 11;
const VIRTIO_BLK_T_WRITE_ZEROES: u32 = 13;

/// Virtio block related errors.
#[derive(Debug)]
pub enum Error {
    /// Guest gave us too few descriptors in a descriptor chain.
    DescriptorChainTooShort,
    /// Guest gave us a descriptor that was too short to use.
    DescriptorLengthTooSmall,
    /// Guest gave us bad memory addresses.
    GuestMemory(GuestMemoryError),
    /// Guest gave us a read only descriptor that protocol says to write to.
    UnexpectedReadOnlyDescriptor,
    /// Guest gave us a write only descriptor that protocol says to read from.
    UnexpectedWriteOnlyDescriptor,
}

/// Type of request from driver to device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequestType {
    /// Read request.
    In,
    /// Write request.
    Out,
    /// Flush request.
    Flush,
    /// Discard request.
    Discard,
    /// Write zeroes request.
    WriteZeroes,
    /// Unknown request.
    Unsupported(u32),
}

impl From<u32> for RequestType {
    fn from(value: u32) -> Self {
        match value {
            VIRTIO_BLK_T_IN => RequestType::In,
            VIRTIO_BLK_T_OUT => RequestType::Out,
            VIRTIO_BLK_T_FLUSH => RequestType::Flush,
            VIRTIO_BLK_T_DISCARD => RequestType::Discard,
            VIRTIO_BLK_T_WRITE_ZEROES => RequestType::WriteZeroes,
            t => RequestType::Unsupported(t),
        }
    }
}

/// Request header.
#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct RequestHeader {
    request_type: u32,
    _reserved: u32,
    sector: u64,
}

/// something.
pub struct Request {
    request_type: RequestType,
    data_len: u32,
    status_addr: GuestAddress,
    sector: u64,
    data_addr: GuestAddress,
}

// Safe because RequestHeader only contains plain data.
unsafe impl ByteValued for RequestHeader {}

impl Request {
    /// Returns the request type.
    pub fn request_type(&self) -> RequestType {
        self.request_type
    }

    /// Returns the data len.
    pub fn data_len(&self) -> u32 {
        self.data_len
    }

    /// Returns the status address.
    pub fn status_addr(&self) -> GuestAddress {
        self.status_addr
    }

    /// Parses a request.
    pub fn parse<M: GuestMemory>(
        desc_chain: &mut DescriptorChain<M>,
        mem: &M,
    ) -> result::Result<Request, Error> {
        let avail_desc = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;
        // The head contains the request type which MUST be readable.
        if avail_desc.is_write_only() {
            return Err(Error::UnexpectedWriteOnlyDescriptor);
        }

        let request_header = mem
            .read_obj::<RequestHeader>(avail_desc.addr())
            .map_err(Error::GuestMemory)?;
        let mut req = Request {
            request_type: RequestType::from(request_header.request_type),
            sector: request_header.sector,
            data_addr: GuestAddress(0),
            data_len: 0,
            status_addr: GuestAddress(0),
        };

        let data_desc;
        let status_desc;
        let desc = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;

        if !desc.has_next() {
            status_desc = desc;
            // Only flush requests are allowed to skip the data descriptor.
            if req.request_type != RequestType::Flush {
                return Err(Error::DescriptorChainTooShort);
            }
        } else {
            data_desc = desc;
            status_desc = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;

            if data_desc.is_write_only() && req.request_type == RequestType::Out {
                return Err(Error::UnexpectedWriteOnlyDescriptor);
            }
            if !data_desc.is_write_only() && req.request_type == RequestType::In {
                return Err(Error::UnexpectedReadOnlyDescriptor);
            }

            // Check that the address of the data descriptor is valid in guest memory.
            let _ = mem
                .checked_offset(data_desc.addr(), data_desc.len() as usize)
                .ok_or(Error::GuestMemory(GuestMemoryError::InvalidGuestAddress(
                    data_desc.addr(),
                )))?;

            req.data_addr = data_desc.addr();
            req.data_len = data_desc.len();
        }

        // The status MUST always be writable.
        if !status_desc.is_write_only() {
            return Err(Error::UnexpectedReadOnlyDescriptor);
        }

        if status_desc.len() < 1 {
            return Err(Error::DescriptorLengthTooSmall);
        }

        // Check that the address of the status descriptor is valid in guest memory.
        // We will write an u32 status here after executing the request.
        let _ = mem
            .checked_offset(status_desc.addr(), mem::size_of::<u32>())
            .ok_or(Error::GuestMemory(GuestMemoryError::InvalidGuestAddress(
                status_desc.addr(),
            )))?;

        req.status_addr = status_desc.addr();

        Ok(req)
    }
}
