// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
//
// SPDX-License-Identifier: Apache-2.0 AND BSD-3-Clause

/// Virtio block request.
///
/// TODO: add more details.
use std::{mem, result};

use crate::queue::DescriptorChain;
use vm_memory::{
    ByteValued, Bytes, GuestAddress, GuestAddressSpace, GuestMemory, GuestMemoryError,
};

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

/// Stores the necessary information for further execution of a block request.
pub struct Request {
    /// The type of the request.
    request_type: RequestType,
    /// Stores the (address, data length) pairs where the data descriptors
    /// point to.
    data_descriptors: Vec<(GuestAddress, u32)>,
    /// The offset (multiplied by 512) where the read or write is to occur.
    sector: u64,
    /// The address where the device should write the request status.
    status_addr: GuestAddress,
}

// Safe because RequestHeader contains only plain data.
unsafe impl ByteValued for RequestHeader {}

impl Request {
    /// Returns the request type.
    pub fn request_type(&self) -> RequestType {
        self.request_type
    }

    /// Returns the data descriptors' (address, len) pairs.
    pub fn data_descriptors(&self) -> &Vec<(GuestAddress, u32)> {
        &self.data_descriptors
    }

    /// Returns the sector.
    pub fn sector(&self) -> u64 {
        self.sector
    }

    /// Returns the status address.
    pub fn status_addr(&self) -> GuestAddress {
        self.status_addr
    }

    /// Parses a request from a given `desc_chain`.
    pub fn parse<M: GuestAddressSpace>(
        desc_chain: &mut DescriptorChain<M>,
    ) -> result::Result<Request, Error> {
        let chain_head = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;
        // The head contains the request type which MUST be readable.
        if chain_head.is_write_only() {
            return Err(Error::UnexpectedWriteOnlyDescriptor);
        }

        let request_header = desc_chain
            .memory()
            .read_obj::<RequestHeader>(chain_head.addr())
            .map_err(Error::GuestMemory)?;

        let mut request = Request {
            request_type: RequestType::from(request_header.request_type),
            data_descriptors: Vec::new(),
            sector: request_header.sector,
            status_addr: GuestAddress(0),
        };

        let status_desc;
        let mut desc = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;

        if !desc.has_next() {
            status_desc = desc;
            // Only flush requests are allowed to skip the data descriptor(s).
            if request.request_type != RequestType::Flush {
                return Err(Error::DescriptorChainTooShort);
            }
        } else {
            while desc.has_next() {
                if desc.is_write_only() && request.request_type == RequestType::Out {
                    return Err(Error::UnexpectedWriteOnlyDescriptor);
                }
                if !desc.is_write_only() && request.request_type == RequestType::In {
                    return Err(Error::UnexpectedReadOnlyDescriptor);
                }
                // TODO check if such checks make sense for discard/write zeroes.

                // Check that the address of the data descriptor is valid in guest memory.
                let _ = desc_chain
                    .memory()
                    .checked_offset(desc.addr(), desc.len() as usize)
                    .ok_or(Error::GuestMemory(GuestMemoryError::InvalidGuestAddress(
                        desc.addr(),
                    )))?;

                request.data_descriptors.push((desc.addr(), desc.len()));
                desc = desc_chain.next().ok_or(Error::DescriptorChainTooShort)?;
            }
            status_desc = desc;
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
        let _ = desc_chain
            .memory()
            .checked_offset(status_desc.addr(), mem::size_of::<u32>())
            .ok_or(Error::GuestMemory(GuestMemoryError::InvalidGuestAddress(
                status_desc.addr(),
            )))?;

        request.status_addr = status_desc.addr();

        Ok(request)
    }
}
