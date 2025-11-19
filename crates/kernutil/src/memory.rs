use heapless::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryDescriptor {
    pub name: &'static str,
    pub physical_start: usize,
    pub size_in_bytes: usize,
    pub memory_type: MemoryType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Usable,
    Reserved,
}

pub fn cal_free_memories(
    free: &[MemoryDescriptor],
    rsv: &[MemoryDescriptor],
    page_size: usize,
) -> Vec<MemoryDescriptor, 64> {
    let filtered_rsv = filter_reserved_overlaps(free, rsv);
    let aligned_rsv = align_reserved_regions(&filtered_rsv, page_size);
    let mut split_ram = split_ram_regions(free, &aligned_rsv);

    sort_by_physical_start(&mut split_ram);
    let merged_segments = merge_contiguous_free_segments(split_ram);

    let mut result = Vec::<MemoryDescriptor, 64>::new();
    for descriptor in merged_segments.into_iter() {
        if result.push(descriptor).is_err() {
            break;
        }
    }

    result
}

/// 判断区间段是否与任何保留区域相交
fn determine_segment_type(
    segment_start: usize,
    segment_end: usize,
    rsv: &[MemoryDescriptor],
) -> MemoryType {
    for reserved in rsv {
        let rsv_start = reserved.physical_start;
        let rsv_end = reserved.physical_start + reserved.size_in_bytes;

        // 检查是否相交
        if segment_start < rsv_end && segment_end > rsv_start {
            return MemoryType::Reserved;
        }
    }

    MemoryType::Usable
}

fn filter_reserved_overlaps(
    free: &[MemoryDescriptor],
    rsv: &[MemoryDescriptor],
) -> heapless::Vec<MemoryDescriptor, 64> {
    let mut filtered = heapless::Vec::<MemoryDescriptor, 64>::new();

    for reserved in rsv {
        let rsv_start = reserved.physical_start;
        let rsv_end = reserved.physical_start + reserved.size_in_bytes;

        if overlaps_any_free(rsv_start, rsv_end, free) && filtered.push(*reserved).is_err() {
            break;
        }
    }

    filtered
}

fn overlaps_any_free(start: usize, end: usize, free: &[MemoryDescriptor]) -> bool {
    for region in free {
        let region_start = region.physical_start;
        let region_end = region.physical_start + region.size_in_bytes;

        if start < region_end && end > region_start {
            return true;
        }
    }

    false
}

/// 对保留区域进行页面对齐扩展
fn align_reserved_regions(
    rsv: &[MemoryDescriptor],
    page_size: usize,
) -> heapless::Vec<MemoryDescriptor, 64> {
    let mut aligned_rsv = heapless::Vec::<MemoryDescriptor, 64>::new();

    for reserved in rsv {
        // 向前对齐start地址
        let aligned_start = (reserved.physical_start / page_size) * page_size;

        // 向后对齐end地址
        let end = reserved.physical_start + reserved.size_in_bytes;
        let aligned_end = end.div_ceil(page_size) * page_size;

        let aligned_size = aligned_end - aligned_start;

        if aligned_size > 0 {
            let mut aligned_descriptor = *reserved;
            aligned_descriptor.physical_start = aligned_start;
            aligned_descriptor.size_in_bytes = aligned_size;

            if aligned_rsv.push(aligned_descriptor).is_err() {
                break;
            }
        }
    }

    aligned_rsv
}

fn split_ram_regions(
    ram: &[MemoryDescriptor],
    rsv: &[MemoryDescriptor],
) -> Vec<MemoryDescriptor, 128> {
    let mut segments = Vec::<MemoryDescriptor, 128>::new();

    for region in ram {
        if region.size_in_bytes == 0 {
            continue;
        }

        let region_start = region.physical_start;
        let region_end = region_start + region.size_in_bytes;
        let mut boundaries = heapless::Vec::<usize, 64>::new();
        let _ = boundaries.push(region_start);
        let _ = boundaries.push(region_end);

        for reserved in rsv {
            let overlap_start = region_start.max(reserved.physical_start);
            let overlap_end = region_end.min(reserved.physical_start + reserved.size_in_bytes);

            if overlap_start < overlap_end {
                let _ = boundaries.push(overlap_start);
                let _ = boundaries.push(overlap_end);
            }
        }

        if boundaries.len() < 2 {
            continue;
        }

        sort_usize_vec(&mut boundaries);
        let unique_boundaries = dedup_sorted(boundaries);

        if unique_boundaries.len() < 2 {
            continue;
        }

        for idx in 0..unique_boundaries.len() - 1 {
            let start = unique_boundaries[idx];
            let end = unique_boundaries[idx + 1];

            if start == end {
                continue;
            }

            let classification = determine_segment_type(start, end, rsv);
            let mut descriptor = *region;
            descriptor.physical_start = start;
            descriptor.size_in_bytes = end - start;
            descriptor.memory_type = classification;

            if descriptor.size_in_bytes == 0 {
                continue;
            }

            if descriptor.memory_type == MemoryType::Reserved {
                continue;
            }

            if segments.push(descriptor).is_err() {
                return segments;
            }
        }
    }

    segments
}

fn sort_usize_vec<const N: usize>(values: &mut heapless::Vec<usize, N>) {
    for i in 0..values.len() {
        for j in i + 1..values.len() {
            if values[i] > values[j] {
                values.swap(i, j);
            }
        }
    }
}

fn dedup_sorted<const N: usize>(values: heapless::Vec<usize, N>) -> heapless::Vec<usize, N> {
    let mut deduped = heapless::Vec::<usize, N>::new();

    for value in values.into_iter() {
        if deduped.last() != Some(&value) {
            let _ = deduped.push(value);
        }
    }

    deduped
}

fn sort_by_physical_start<const N: usize>(segments: &mut Vec<MemoryDescriptor, N>) {
    for i in 0..segments.len() {
        for j in i + 1..segments.len() {
            if segments[i].physical_start > segments[j].physical_start {
                segments.swap(i, j);
            }
        }
    }
}

fn merge_contiguous_free_segments<const N: usize>(
    segments: Vec<MemoryDescriptor, N>,
) -> Vec<MemoryDescriptor, N> {
    let mut merged = Vec::<MemoryDescriptor, N>::new();

    for segment in segments.into_iter() {
        if segment.size_in_bytes == 0 {
            continue;
        }

        if match merged.last_mut() {
            Some(last) if should_merge_free(last, &segment) => {
                last.size_in_bytes += segment.size_in_bytes;
                true
            }
            _ => false,
        } {
            continue;
        }

        if merged.push(segment).is_err() {
            break;
        }
    }

    merged
}

fn should_merge_free(prev: &MemoryDescriptor, next: &MemoryDescriptor) -> bool {
    prev.memory_type == MemoryType::Usable
        && next.memory_type == MemoryType::Usable
        && prev.physical_start + prev.size_in_bytes == next.physical_start
}

#[cfg(all(not(target_os = "none"), test))]
mod test {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec as StdVec;

    const PAGE_SIZE: usize = 4096;

    fn desc(name: &'static str, start: usize, size: usize, ty: MemoryType) -> MemoryDescriptor {
        MemoryDescriptor {
            name,
            physical_start: start,
            size_in_bytes: size,
            memory_type: ty,
        }
    }

    #[test]
    fn splits_ram_segments_when_reserved_inside() {
        let ram: StdVec<MemoryDescriptor> = vec![desc("ram", 0x1000, 0x4000, MemoryType::Usable)];
        let rsv: StdVec<MemoryDescriptor> = vec![desc("rsv", 0x2000, 0x1000, MemoryType::Reserved)];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], desc("ram", 0x1000, 0x1000, MemoryType::Usable));
        assert_eq!(result[1], desc("ram", 0x3000, 0x2000, MemoryType::Usable));
    }

    #[test]
    fn keeps_ram_only_when_no_reserved_overlap() {
        let ram: StdVec<MemoryDescriptor> = vec![
            desc("ram_high", 0x5000, 0x0800, MemoryType::Usable),
            desc("ram_low", 0x1000, 0x1000, MemoryType::Usable),
        ];
        let rsv: StdVec<MemoryDescriptor> = vec![];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].physical_start, 0x1000);
        assert_eq!(result[1].physical_start, 0x5000);
        assert_eq!(result[0].memory_type, MemoryType::Usable);
        assert_eq!(result[1].memory_type, MemoryType::Usable);
    }

    #[test]
    fn preserves_reserved_regions_outside_ram() {
        let ram: StdVec<MemoryDescriptor> = vec![desc("ram", 0x1000, 0x2000, MemoryType::Usable)];
        let rsv: StdVec<MemoryDescriptor> = vec![desc("rsv", 0x5000, 0x1000, MemoryType::Reserved)];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], desc("ram", 0x1000, 0x2000, MemoryType::Usable));
    }

    #[test]
    fn removes_fully_reserved_ram_segments() {
        let ram: StdVec<MemoryDescriptor> = vec![desc("ram", 0x2000, 0x2000, MemoryType::Usable)];
        let rsv: StdVec<MemoryDescriptor> = vec![desc("rsv", 0x1000, 0x4000, MemoryType::Reserved)];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert!(result.is_empty());
    }

    #[test]
    fn splits_multiple_overlaps_and_retains_order() {
        let ram: StdVec<MemoryDescriptor> = vec![desc("ram", 0x0000, 0x6000, MemoryType::Usable)];
        let rsv: StdVec<MemoryDescriptor> = vec![
            desc("rsv0", 0x1000, 0x0800, MemoryType::Reserved),
            desc("rsv1", 0x3000, 0x0800, MemoryType::Reserved),
        ];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], desc("ram", 0x0000, 0x1000, MemoryType::Usable));
        assert_eq!(result[1], desc("ram", 0x2000, 0x1000, MemoryType::Usable));
        assert_eq!(result[2], desc("ram", 0x4000, 0x2000, MemoryType::Usable));
    }

    #[test]
    fn merges_adjacent_free_segments() {
        let ram: StdVec<MemoryDescriptor> = vec![
            desc("ram_low", 0x0000, 0x2000, MemoryType::Usable),
            desc("ram_high", 0x2000, 0x2000, MemoryType::Usable),
        ];
        let rsv: StdVec<MemoryDescriptor> = vec![desc("rsv", 0x4000, 0x1000, MemoryType::Reserved)];

        let result: StdVec<_> = cal_free_memories(&ram, &rsv, PAGE_SIZE)
            .into_iter()
            .collect();

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            desc("ram_low", 0x0000, 0x4000, MemoryType::Usable)
        );
    }
}
