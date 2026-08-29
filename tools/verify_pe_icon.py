"""Verify that a PE executable contains the expected application icon group."""

from pathlib import Path
import struct
import sys


RT_ICON = 3
RT_GROUP_ICON = 14
REQUIRED_SIZES = {16, 20, 24, 32, 40, 48, 64, 128, 256}


def u16(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def rva_to_offset(data, sections, rva):
    for virtual_size, virtual_address, raw_size, raw_pointer in sections:
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            offset = raw_pointer + (rva - virtual_address)
            if offset < len(data):
                return offset
    raise ValueError(f"RVA 0x{rva:x} is outside PE sections")


def parse_directory(data, resource_base, relative_offset):
    offset = resource_base + relative_offset
    named = u16(data, offset + 12)
    ids = u16(data, offset + 14)
    entries = {}
    for index in range(named + ids):
        entry = offset + 16 + index * 8
        name = u32(data, entry)
        key = name & 0x7FFFFFFF
        child = u32(data, entry + 4)
        if child & 0x80000000:
            value = parse_directory(data, resource_base, child & 0x7FFFFFFF)
        else:
            data_entry = resource_base + (child & 0x7FFFFFFF)
            payload_rva = u32(data, data_entry)
            payload_size = u32(data, data_entry + 4)
            value = (payload_rva, payload_size)
        entries[key] = value
    return entries


def first_leaf(node):
    if isinstance(node, tuple):
        return node
    if not isinstance(node, dict) or not node:
        raise ValueError("resource directory has no data leaf")
    return first_leaf(next(iter(node.values())))


def read_pe_sections(data):
    if data[:2] != b"MZ":
        raise ValueError("missing DOS header")
    nt = u32(data, 0x3C)
    if data[nt : nt + 4] != b"PE\0\0":
        raise ValueError("missing PE signature")
    file_header = nt + 4
    section_count = u16(data, file_header + 2)
    optional_size = u16(data, file_header + 16)
    optional = file_header + 20
    magic = u16(data, optional)
    if magic == 0x10B:
        directory_offset = optional + 96
    elif magic == 0x20B:
        directory_offset = optional + 112
    else:
        raise ValueError(f"unsupported optional-header magic 0x{magic:x}")
    resource_rva = u32(data, directory_offset + 8 * 2)
    resource_size = u32(data, directory_offset + 8 * 2 + 4)
    section_table = optional + optional_size
    sections = []
    for index in range(section_count):
        section = section_table + index * 40
        sections.append(
            (
                u32(data, section + 8),
                u32(data, section + 12),
                u32(data, section + 16),
                u32(data, section + 20),
            )
        )
    resource_base = rva_to_offset(data, sections, resource_rva)
    if resource_size == 0:
        raise ValueError("resource directory is empty")
    return sections, resource_base


def verify(path):
    data = Path(path).read_bytes()
    sections, resource_base = read_pe_sections(data)
    resources = parse_directory(data, resource_base, 0)
    group_type = resources.get(RT_GROUP_ICON)
    icon_type = resources.get(RT_ICON)
    if group_type is None or icon_type is None:
        raise ValueError("missing RT_GROUP_ICON or RT_ICON resource")

    group_payload_rva, group_payload_size = first_leaf(group_type.get(1))
    group_offset = rva_to_offset(data, sections, group_payload_rva)
    if group_payload_size < 6:
        raise ValueError("icon group payload is truncated")
    reserved, icon_type_value, count = struct.unpack_from("<HHH", data, group_offset)
    if reserved != 0 or icon_type_value != 1 or count == 0:
        raise ValueError("invalid icon group header")

    sizes = set()
    icon_ids = []
    for index in range(count):
        entry = group_offset + 6 + index * 14
        if entry + 14 > group_offset + group_payload_size:
            raise ValueError("icon group entry is truncated")
        width = data[entry] or 256
        height = data[entry + 1] or 256
        planes = u16(data, entry + 4)
        bit_count = u16(data, entry + 6)
        icon_id = u16(data, entry + 12)
        sizes.add((width, height))
        icon_ids.append(icon_id)
        if planes != 1 or bit_count != 32:
            raise ValueError(f"unexpected icon format for {width}x{height}")

    missing = {(size, size) for size in REQUIRED_SIZES} - sizes
    if missing:
        raise ValueError(f"icon group is missing sizes: {sorted(missing)}")

    for icon_id in icon_ids:
        payload = icon_type.get(icon_id)
        if payload is None:
            raise ValueError(f"icon group references missing RT_ICON id {icon_id}")
        payload_rva, payload_size = first_leaf(payload)
        icon_offset = rva_to_offset(data, sections, payload_rva)
        if payload_size < 40 or u32(data, icon_offset) != 40:
            raise ValueError(f"RT_ICON id {icon_id} has invalid DIB data")

    print(f"icon resource OK: {count} images, sizes={sorted(sizes)}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify_pe_icon.py PATH")
    try:
        verify(sys.argv[1])
    except (OSError, struct.error, ValueError) as error:
        raise SystemExit(f"icon resource verification failed: {error}")
