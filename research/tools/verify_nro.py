#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Offline sanity check for a Horizon NRO's dynamic relocations.

It maps the three NRO segments plus BSS into a simulated address space,
resolves the dynamic symbol table the way a loader would (external/undefined
weak symbols become 0, imports get fake addresses), applies every relocation
and records the segment each relocation writes to.

A file that writes to its RX text or RO rodata is what crashes on real Switch
hardware.  The hardware-ready damage_numbers.nro has zero such writes.
"""

import struct
import sys


class Nro:
    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        assert self.data[0x10:0x14] == b"NRO0", "not an NRO"
        self.size = struct.unpack_from("<I", self.data, 0x18)[0]
        self.text_off, self.text_sz = struct.unpack_from("<2I", self.data, 0x20)
        self.ro_off, self.ro_sz = struct.unpack_from("<2I", self.data, 0x28)
        self.rw_off, self.rw_sz = struct.unpack_from("<2I", self.data, 0x30)
        self.bss_sz = struct.unpack_from("<I", self.data, 0x38)[0]
        mod0 = struct.unpack_from("<I", self.data, 0x04)[0]
        assert self.data[mod0:mod0 + 4] == b"MOD0"
        dyn_off = struct.unpack_from("<I", self.data, mod0 + 4)[0]
        self.dyn = mod0 + dyn_off
        self.tags = {}
        self.tag_order = []
        for i in range(128):
            off = self.dyn + i * 8
            tag, val = struct.unpack_from("<iI", self.data, off)
            self.tag_order.append((tag, val))
            self.tags.setdefault(tag, []).append(val)
            if tag == 0:
                break

    @property
    def end(self):
        return self.rw_off + self.rw_sz + self.bss_sz


def segment_of(nro, off):
    if off < nro.text_off + nro.text_sz:
        return "text"
    if off < nro.ro_off + nro.ro_sz:
        return "rodata"
    if off < nro.rw_off + nro.rw_sz:
        return "data"
    if off < nro.end:
        return "bss"
    return "outside"


def check(path, base=0x10000000):
    nro = Nro(path)
    text_end = nro.text_off + nro.text_sz
    ro_end = nro.ro_off + nro.ro_sz
    rw_end = nro.rw_off + nro.rw_sz

    # simulated memory, indexed by module-relative offset
    mem = bytearray(nro.end)
    for off, seg_off, seg_sz in (
        (nro.text_off, nro.text_off, nro.text_sz),
        (nro.ro_off, nro.ro_off, nro.ro_sz),
        (nro.rw_off, nro.rw_off, nro.rw_sz),
    ):
        mem[off:off + seg_sz] = nro.data[seg_off:seg_off + seg_sz]

    # dynamic symbol table
    strtab = nro.tags[5][0]
    symtab = nro.tags[6][0]
    strsz = nro.tags[10][0]
    syment = nro.tags[11][0]
    hash_off = nro.tags[4][0]
    nbucket, nchain = struct.unpack_from("<II", nro.data, hash_off)
    syms = []
    for i in range(nchain):
        name_off, value, size, info, other, shndx = struct.unpack_from(
            "<IIIBBH", nro.data, symtab + i * syment)
        name = ""
        if name_off:
            end = nro.data.index(b"\0", strtab + name_off)
            name = nro.data[strtab + name_off:end].decode("utf-8", "replace")
        syms.append((name, value, info, shndx))

    # resolve: defined -> base+value, weak undefined -> 0, imports -> fake addr
    resolved = []
    fake = 0x20000000
    for name, value, info, shndx in syms:
        bind = info >> 4
        if shndx != 0:
            addr = base + value
        elif bind == 2:            # STB_WEAK undefined
            addr = 0
        else:
            addr = fake
            fake += 0x100
        resolved.append(addr)

    writes = {}
    errors = []
    for tag in (17, 23):           # DT_REL, DT_JMPREL
        if tag not in nro.tags:
            continue
        off = nro.tags[tag][0]
        sz = nro.tags[18][0] if tag == 17 else nro.tags[2][0]
        for i in range(sz // 8):
            ent = off + i * 8
            r_off, r_info = struct.unpack_from("<II", nro.data, ent)
            sym = r_info >> 8
            typ = r_info & 0xFF
            if r_off >= len(mem):
                errors.append("reloc %d: target 0x%x outside module" % (i, r_off))
                continue
            if typ == 23:          # RELATIVE: *target += base
                addend = struct.unpack_from("<I", mem, r_off)[0]
                value = (base + addend) & 0xFFFFFFFF
            elif typ in (2, 21, 22):   # ABS32 / GLOB_DAT / JUMP_SLOT
                value = (resolved[sym] + struct.unpack_from("<I", mem, r_off)[0]) & 0xFFFFFFFF if typ == 2 else resolved[sym]
            else:
                errors.append("reloc %d: unknown type %d" % (i, typ))
                continue
            seg = segment_of(nro, r_off)
            writes[seg] = writes.get(seg, 0) + 1
            if seg in ("text", "rodata"):
                errors.append("reloc %d (type %d) writes %s at 0x%x" % (i, typ, seg, r_off))
            struct.pack_into("<I", mem, r_off, value)

    init_arr = struct.unpack_from("<I", mem, nro.tags[25][0])[0] if 25 in nro.tags else None
    fini_arr = struct.unpack_from("<I", mem, nro.tags[26][0])[0] if 26 in nro.tags else None
    textrel = 22 in nro.tags

    print("== %s" % path)
    print("   text=0x%x ro=0x%x data=0x%x bss=0x%x end=0x%x" %
          (nro.text_sz, nro.ro_sz, nro.rw_sz, nro.bss_sz, nro.end))
    print("   DT_TEXTREL present: %s" % textrel)
    print("   reloc writes by segment: %s" % writes)
    if init_arr is not None:
        print("   init_array -> 0x%x, fini_array -> 0x%x" % (init_arr, fini_arr))
    if errors:
        print("   FAIL: %d illegal writes" % len(errors))
        for e in errors[:16]:
            print("     -", e)
        return False
    print("   OK: every relocation targets writable memory")
    return True


if __name__ == "__main__":
    ok = True
    for p in sys.argv[1:]:
        ok = check(p) and ok
    sys.exit(0 if ok else 1)
