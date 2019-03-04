use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::structures::paging::{
    mapper::{TranslateError},
    FrameAllocator, Mapper, MappedPageTable, Page, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};
use x86_64::registers::control::Cr3;

/// Creates a Mapper instance from the level 4 address.
///
/// TODO: call only once.
pub unsafe fn init(physical_memory_offset: u64) -> impl Mapper<Size4KiB> {
    /// Rust currently treats the whole body of unsafe functions as an unsafe
    /// block, which makes it difficult to see which operations are unsafe. To
    /// limit the scope of unsafe we use a safe inner function.
    fn init_inner(physical_memory_offset: u64) -> impl Mapper<Size4KiB> {
        let phys_to_virt = move |frame: PhysFrame| {
            let phys_addr = frame.start_address().as_u64();
            let virt_addr = VirtAddr::new(phys_addr + physical_memory_offset);
            virt_addr.as_mut_ptr()
        };

        let level_4_table = {
            let (frame, _) = Cr3::read();
            unsafe { &mut *phys_to_virt(frame) }
        };

        unsafe { MappedPageTable::new(level_4_table, phys_to_virt) }
    }

    init_inner(physical_memory_offset)
}

/// Create a FrameAllocator from the passed memory map
pub fn init_frame_allocator(
    memory_map: &'static MemoryMap,
) -> BootInfoFrameAllocator<impl Iterator<Item = PhysFrame>> {
    // get usable regions from memory map
    let regions = memory_map
        .iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable);
    // map each region to its address range
    let addr_ranges = regions.map(|r| r.range.start_addr()..r.range.end_addr());
    // transform to an iterator of frame start addresses
    let frame_addresses = addr_ranges.flat_map(|r| r.into_iter().step_by(4096));
    // create `PhysFrame` types from the start addresses
    let frames = frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)));

    BootInfoFrameAllocator { frames }
}

/// Returns the physical address for the given virtual address, or `None` if
/// the virtual address is not mapped.
pub fn translate_addr(
    addr: u64,
    mapper: &impl Mapper<Size4KiB>,
) -> Result<PhysAddr, TranslateError> {
    let addr = VirtAddr::new(addr);
    let page: Page = Page::containing_address(addr);

    // perform the translation
    let frame = mapper.translate_page(page);
    frame.map(|frame| frame.start_address() + u64::from(addr.page_offset()))
}

pub fn create_example_mapping(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let page: Page = Page::containing_address(VirtAddr::new(0xdeadbeaf000));
    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    let flags = Flags::PRESENT | Flags::WRITABLE;

    let map_to_result = unsafe { mapper.map_to(page, frame, flags, frame_allocator) };
    map_to_result.expect("map_to failed").flush();
}

/// A FrameAllocator that always returns `None`.
pub struct EmptyFrameAllocator;

impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        None
    }
}

pub struct BootInfoFrameAllocator<I>
where
    I: Iterator<Item = PhysFrame>,
{
    frames: I,
}

impl<I> FrameAllocator<Size4KiB> for BootInfoFrameAllocator<I>
where
    I: Iterator<Item = PhysFrame>,
{
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.frames.next()
    }
}
