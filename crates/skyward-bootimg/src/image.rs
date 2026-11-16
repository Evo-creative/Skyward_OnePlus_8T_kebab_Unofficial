use abootimg_oxide::{binrw::io::Cursor, Header, HeaderV0Versioned};
use anyhow::{anyhow, ensure, Context, Result};
use std::fs;
use std::path::Path;

/// A boot image with header version 0 through 4 (`boot`, `init_boot`,
/// `recovery`) split into its header and the sections after it: the kernel,
/// the ramdisk, the second stage bootloader, the recovery DTBO and the DTB,
/// as many of them as the header's version has.
pub struct BootImage {
    pub header: Header,
    pub kernel: Vec<u8>,
    pub ramdisk: Vec<u8>,
    /// The second stage bootloader, header versions 0 through 2 only.
    pub second: Vec<u8>,
    /// The recovery DTBO, header versions 1 and 2 only.
    pub recovery_dtbo: Vec<u8>,
    /// The device tree blob, header version 2 only.
    pub dtb: Vec<u8>,
}

/// Where a header's sections are and how long they are: what version 0
/// through 2 headers say, each section padded to the header's page size.
struct Layout {
    kernel: usize,
    ramdisk: usize,
    second: usize,
    recovery_dtbo: usize,
    dtb: usize,
    second_size: u32,
    recovery_dtbo_size: u32,
    dtb_size: u32,
}

impl Layout {
    fn of(header: &Header) -> Result<Self> {
        let (second, recovery_dtbo, dtb, second_size, recovery_dtbo_size, dtb_size) = match header {
            Header::V0(h) => {
                let page = h.page_size as usize;
                ensure!(page > 0, "the header names a page size of 0");
                let second = h.second_bootloader_position();
                let second_size = h.second_bootloader_size;
                // Version 1 has a recovery DTBO; version 2 has that and a DTB.
                // Version 0 has neither, and its versioned part says so.
                let (recovery_dtbo_size, dtb_size) = match h.versioned {
                    HeaderV0Versioned::V0 => (0, 0),
                    HeaderV0Versioned::V1 { recovery_dtbo_size, .. } => (recovery_dtbo_size, 0),
                    HeaderV0Versioned::V2 { recovery_dtbo_size, dtb_size, .. } => (recovery_dtbo_size, dtb_size),
                };
                let recovery_dtbo = second + second_size as usize + padding(second_size as usize, page);
                let dtb = recovery_dtbo + recovery_dtbo_size as usize + padding(recovery_dtbo_size as usize, page);
                (second, recovery_dtbo, dtb, second_size, recovery_dtbo_size, dtb_size)
            }
            // From version 3 on, the kernel sits at the first page and the
            // ramdisk after it; a version 4 boot signature follows the
            // ramdisk and is not kept.
            Header::V3(h) => (h.ramdisk_position(), 0, 0, 0, 0, 0),
        };
        Ok(Layout {
            kernel: header.kernel_position(),
            ramdisk: header.ramdisk_position(),
            second,
            recovery_dtbo,
            dtb,
            second_size,
            recovery_dtbo_size,
            dtb_size,
        })
    }
}

/// How many bytes of padding take a section of `len` bytes to the next page.
fn padding(len: usize, page: usize) -> usize {
    (page - (len % page)) % page
}

fn section(bytes: &[u8], at: usize, len: u32, what: &str) -> Result<Vec<u8>> {
    let end = at.checked_add(len as usize).ok_or_else(|| anyhow!("{what} runs past the end of the image"))?;
    bytes.get(at..end).map(<[u8]>::to_vec).ok_or_else(|| anyhow!("{what} runs past the end of the image"))
}

impl BootImage {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        BootImage::parse(&bytes).with_context(|| format!("read {}", path.display()))
    }

    /// The image `bytes` hold, as a partition holds it: the image, then
    /// whatever fills the partition after it.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let header = Header::parse(&mut Cursor::new(bytes)).context("header")?;
        let layout = Layout::of(&header)?;
        let kernel = section(bytes, layout.kernel, header.kernel_size(), "kernel")?;
        let ramdisk = section(bytes, layout.ramdisk, header.ramdisk_size(), "ramdisk")?;
        let second = section(bytes, layout.second, layout.second_size, "second stage bootloader")?;
        let recovery_dtbo = section(bytes, layout.recovery_dtbo, layout.recovery_dtbo_size, "recovery DTBO")?;
        let dtb = section(bytes, layout.dtb, layout.dtb_size, "DTB")?;
        Ok(BootImage { header, kernel, ramdisk, second, recovery_dtbo, dtb })
    }

    /// The image with sizes taken from its parts. A version 4 header keeps its
    /// version but carries no boot signature; unlocked bootloaders do not ask
    /// for one.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut header = self.header.clone();
        match &mut header {
            Header::V0(h) => {
                h.kernel_size = u32::try_from(self.kernel.len()).context("kernel size")?;
                h.ramdisk_size = u32::try_from(self.ramdisk.len()).context("ramdisk size")?;
                h.second_bootloader_size = u32::try_from(self.second.len()).context("second stage bootloader size")?;
                match &mut h.versioned {
                    HeaderV0Versioned::V0 => {
                        ensure!(self.second.is_empty(), "a version 0 header has no second stage bootloader");
                        ensure!(
                            self.recovery_dtbo.is_empty() && self.dtb.is_empty(),
                            "a version 0 header has no recovery DTBO and no DTB"
                        );
                    }
                    HeaderV0Versioned::V1 { recovery_dtbo_size, .. } => {
                        *recovery_dtbo_size = u32::try_from(self.recovery_dtbo.len()).context("recovery DTBO size")?;
                        ensure!(self.dtb.is_empty(), "a version 1 header has no DTB");
                    }
                    HeaderV0Versioned::V2 { recovery_dtbo_size, dtb_size, .. } => {
                        *recovery_dtbo_size = u32::try_from(self.recovery_dtbo.len()).context("recovery DTBO size")?;
                        *dtb_size = u32::try_from(self.dtb.len()).context("DTB size")?;
                    }
                }
            }
            Header::V3(h) => {
                h.kernel_size = u32::try_from(self.kernel.len()).context("kernel size")?;
                h.ramdisk_size = u32::try_from(self.ramdisk.len()).context("ramdisk size")?;
                h.v4_signature_size = h.v4_signature_size.map(|_| 0);
                ensure!(
                    self.second.is_empty() && self.recovery_dtbo.is_empty() && self.dtb.is_empty(),
                    "a version 3 or 4 header holds no second stage bootloader, recovery DTBO or DTB"
                );
            }
        }
        let page = header.page_size();
        ensure!(page > 0, "the header names a page size of 0");
        let layout = Layout::of(&header)?;
        let mut out = Vec::new();
        header.write(&mut out).context("write header")?;
        pad(&mut out, page);
        out.extend_from_slice(&self.kernel);
        pad(&mut out, page);
        out.extend_from_slice(&self.ramdisk);
        pad(&mut out, page);
        if layout.second_size > 0 {
            out.extend_from_slice(&self.second);
            pad(&mut out, page);
        }
        if layout.recovery_dtbo_size > 0 {
            out.extend_from_slice(&self.recovery_dtbo);
            pad(&mut out, page);
        }
        if layout.dtb_size > 0 {
            out.extend_from_slice(&self.dtb);
            pad(&mut out, page);
        }
        Ok(out)
    }
}

fn pad(out: &mut Vec<u8>, page: usize) {
    out.resize(out.len() + padding(out.len(), page), 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A boot image written the way a bootloader reads one: the header, each
    /// section after it padded to the page size, in the order the header's
    /// version lays them out.
    fn image(version: u32, page: usize, kernel: &[u8], ramdisk: &[u8], second: &[u8], dtbo: &[u8], dtb: &[u8]) -> Vec<u8> {
        let mut header = Vec::new();
        macro_rules! u32le {
            ($n:expr) => {
                header.extend_from_slice(&u32::to_le_bytes($n))
            };
        }
        macro_rules! u64le {
            ($n:expr) => {
                header.extend_from_slice(&u64::to_le_bytes($n))
            };
        }
        header.extend_from_slice(b"ANDROID!");
        u32le!(kernel.len() as u32); // kernel_size
        u32le!(0x8000); // kernel_addr
        u32le!(ramdisk.len() as u32); // ramdisk_size
        u32le!(0x1000000); // ramdisk_addr
        u32le!(second.len() as u32); // second_bootloader_size
        u32le!(0); // second_bootloader_addr
        u32le!(0); // tags_addr
        u32le!(page as u32);
        u32le!(version);
        u32le!(0); // os version
        header.extend_from_slice(&[0; 16]); // board name
        let mut cmdline = [0u8; 512];
        cmdline[..8].copy_from_slice(b"test=one");
        header.extend_from_slice(&cmdline);
        header.extend_from_slice(&[0; 32]); // hash digest
        header.extend_from_slice(&[0; 1024]); // extra command line
        if version >= 1 {
            u32le!(dtbo.len() as u32);
            u64le!(0); // recovery dtbo offset
            u32le!(if version >= 2 { 1660 } else { 1648 });
        }
        if version >= 2 {
            u32le!(dtb.len() as u32);
            u64le!(0x1f00000);
        }
        let mut out = header;
        pad(&mut out, page);
        for part in [kernel, ramdisk, second, dtbo, dtb] {
            out.extend_from_slice(part);
            pad(&mut out, page);
        }
        out
    }

    #[test]
    fn a_version_two_image_keeps_its_kernel_ramdisk_dtbo_and_dtb() {
        let (kernel, ramdisk) = (vec![b'K'; 5000], vec![b'R'; 3000]);
        let (dtbo, dtb) = (vec![b'T'; 900], vec![b'D'; 2500]);
        let bytes = image(2, 4096, &kernel, &ramdisk, &[], &dtbo, &dtb);
        let img = BootImage::parse(&bytes).unwrap();
        assert_eq!(img.header.header_version(), 2);
        assert_eq!(img.kernel, kernel);
        assert_eq!(img.ramdisk, ramdisk);
        assert_eq!(img.dtb, dtb);
        assert_eq!(img.recovery_dtbo, dtbo);
        assert!(img.second.is_empty());
        assert_eq!(img.to_bytes().unwrap(), bytes, "written back as it came");
    }

    #[test]
    fn a_version_one_image_keeps_its_recovery_dtbo_and_no_dtb() {
        let (kernel, ramdisk) = (vec![b'K'; 100], vec![b'R'; 200]);
        let dtbo = vec![b'T'; 700];
        let bytes = image(1, 2048, &kernel, &ramdisk, &[], &dtbo, &[]);
        let img = BootImage::parse(&bytes).unwrap();
        assert_eq!(img.recovery_dtbo, dtbo);
        assert!(img.dtb.is_empty());
        assert_eq!(img.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn a_version_zero_image_holds_kernel_and_ramdisk_alone() {
        let bytes = image(0, 2048, b"kernel", b"ramdisk", &[], &[], &[]);
        let img = BootImage::parse(&bytes).unwrap();
        assert_eq!((img.kernel.as_slice(), img.ramdisk.as_slice()), (b"kernel".as_slice(), b"ramdisk".as_slice()));
        assert_eq!(img.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn the_parts_of_a_version_two_image_are_written_back_in_their_places() {
        let bytes = image(2, 4096, b"kernel", b"ramdisk", b"second", b"dtbo", b"dtb");
        let mut img = BootImage::parse(&bytes).unwrap();
        assert_eq!((img.second.as_slice(), img.recovery_dtbo.as_slice(), img.dtb.as_slice()), (b"second".as_slice(), b"dtbo".as_slice(), b"dtb".as_slice()));
        img.ramdisk = b"a longer ramdisk than before".to_vec();
        let again = BootImage::parse(&img.to_bytes().unwrap()).unwrap();
        assert_eq!(again.kernel, b"kernel");
        assert_eq!(again.ramdisk, b"a longer ramdisk than before");
        assert_eq!(again.second, b"second");
        assert_eq!(again.recovery_dtbo, b"dtbo");
        assert_eq!(again.dtb, b"dtb");
    }

    #[test]
    fn a_version_three_image_still_round_trips() {
        let mut header = Vec::new();
        header.extend_from_slice(b"ANDROID!");
        header.extend_from_slice(&(4u32).to_le_bytes()); // kernel size
        header.extend_from_slice(&(7u32).to_le_bytes()); // ramdisk size
        header.extend_from_slice(&0u32.to_le_bytes()); // os version
        header.extend_from_slice(&1580u32.to_le_bytes()); // header size
        header.extend_from_slice(&[0; 16]);
        header.extend_from_slice(&3u32.to_le_bytes()); // header version
        header.extend_from_slice(&[0; 1536]); // command line
        let mut bytes = header;
        pad(&mut bytes, 4096);
        for part in [b"kern".as_slice(), b"ramdisk"] {
            bytes.extend_from_slice(part);
            pad(&mut bytes, 4096);
        }
        let img = BootImage::parse(&bytes).unwrap();
        assert_eq!(img.header.header_version(), 3);
        assert_eq!((img.kernel.as_slice(), img.ramdisk.as_slice()), (b"kern".as_slice(), b"ramdisk".as_slice()));
        assert_eq!(img.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn a_section_running_past_the_end_is_refused() {
        let mut bytes = image(2, 4096, b"kernel", b"ramdisk", &[], b"dtbo", b"dtb");
        bytes.truncate(bytes.len() - 4096);
        assert!(BootImage::parse(&bytes).is_err());
    }

    #[test]
    fn a_version_zero_header_refuses_a_second_stage_bootloader() {
        let parsed = BootImage::parse(&image(0, 4096, b"k", b"r", &[], &[], &[])).unwrap();
        let img = BootImage { second: b"s".to_vec(), ..parsed };
        assert!(img.to_bytes().is_err());
    }
}