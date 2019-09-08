use std::fs::File;
use std::io::{self, Write};
use std::os::unix::io::FromRawFd;
use std::path::Path;

use futures::{Async, AsyncSink, Poll, Sink, StartSend, Stream};
use nix;
use nix::fcntl;
use nix::libc;
use slog_stdlog;
use slog;
use slog::Drain;
use tokio::reactor::Handle;
use tokio_io::AsyncRead;

use tokio::reactor::PollEvented;
use character_device_file::CharacterDeviceFile;
use character_device::{CharacterDevice, Decoder, Encoder, SyncSink};
use uhid_codec::*;

pub struct UHIDDevice<T> {
    inner: CharacterDevice<T, UHIDCodec, UHIDCodec>,
    logger: slog::Logger,
}

/// Parameters used to create UHID devices
pub struct CreateParams {
    pub name: String,
    pub phys: String,
    pub uniq: String,
    pub bus: Bus,
    pub vendor: u32,
    pub product: u32,
    pub version: u32,
    pub country: u32,
    pub data: Vec<u8>,
}

// ===== impl UHIDDevice =====

impl UHIDDevice<PollEvented<CharacterDeviceFile<File>>> {
    /// Create a UHID device using '/dev/uhid'
    pub fn create<L: Into<Option<slog::Logger>>>(
        handle: &Handle,
        params: CreateParams,
        logger: L,
    ) -> io::Result<UHIDDevice<PollEvented<CharacterDeviceFile<File>>>> {
        Self::create_with_path(Path::new("/dev/uhid"), handle, params, logger)
    }

    /// Create a UHID device, using the specified UHID device file path
    pub fn create_with_path<L: Into<Option<slog::Logger>>>(
        path: &Path,
        handle: &Handle,
        params: CreateParams,
        logger: L,
    ) -> io::Result<UHIDDevice<PollEvented<CharacterDeviceFile<File>>>> {
        let fd = fcntl::open(
            path,
            fcntl::OFlag::from_bits(libc::O_RDWR | libc::O_CLOEXEC | libc::O_NONBLOCK).unwrap(),
            nix::sys::stat::Mode::from_bits(libc::S_IRUSR | libc::S_IWUSR | libc::S_IRGRP | libc::S_IWGRP).unwrap(),
        ).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Cannot open uhid-cdev {}: {}", path.to_str().unwrap(), err),
            )
        })?;
        let file: File = unsafe { File::from_raw_fd(fd) };
        let device_file = CharacterDeviceFile::new(file);
        Ok(Self::create_with(
            device_file.into_io(handle)?,
            params,
            logger,
        ))
    }
}

impl<T> UHIDDevice<T>
where
    T: AsyncRead + Write,
{
    fn create_with<L: Into<Option<slog::Logger>>>(
        inner: T,
        params: CreateParams,
        logger: L,
    ) -> UHIDDevice<T> {
        let logger = logger
            .into()
            .unwrap_or(slog::Logger::root(slog_stdlog::StdLog.fuse(), o!()));
        let mut device = UHIDDevice {
            inner: CharacterDevice::new(inner, UHIDCodec, UHIDCodec, logger.clone()),
            logger,
        };
        debug!(device.logger, "Send create device event");
        device
            .inner
            .send(InputEvent::Create {
                name: params.name,
                phys: params.phys,
                uniq: params.uniq,
                bus: params.bus,
                vendor: params.vendor,
                product: params.product,
                version: params.version,
                country: params.country,
                data: params.data,
            })
            .unwrap();
        debug!(device.logger, "Sent create device event");
        device
    }

    /// Send a HID packet to the UHID device
    pub fn send_input(&mut self, data: &[u8]) -> Result<(), <UHIDCodec as Encoder>::Error> {
        debug!(self.logger, "Send input event");
        self.inner.send(InputEvent::Input {
            data: data.to_vec(),
        })
    }

    /// Send a 'destroy' to the UHID device and close it
    pub fn destroy(mut self) -> Result<(), <UHIDCodec as Encoder>::Error> {
        self.inner.send(InputEvent::Destroy)?;
        self.inner.close()?;
        Ok(())
    }
}

impl<T: AsyncRead> Stream for UHIDDevice<T> {
    type Item = <UHIDCodec as Decoder>::Item;
    type Error = <UHIDCodec as Decoder>::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.inner.poll()
    }
}

impl<T: Write> Sink for UHIDDevice<T> {
    type SinkItem = <UHIDCodec as Encoder>::Item;
    type SinkError = <UHIDCodec as Encoder>::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        debug!(self.logger, "start_send");
        self.inner.send(item)?;
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        debug!(self.logger, "poll_complete");
        self.inner.flush()?;
        Ok(Async::Ready(()))
    }

    fn close(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.inner.close()?;
        Ok(Async::Ready(()))
    }
}
