# Symbol pre-linking

This crate supports ELF loading and symbol preloading. It can be used directly in kernel space (for kernel modules), or it can be used in userspace (for VDSO or other preloading purposes).

# Usage

This crate also can help with loading elf. Checkout src/elf.rs

`no_std` environments are supported, but you need to specify an global alloc.

## VDSO

For VDSO usages, the target symbol must be visible in userspace. This is preferrably done by using a new section (e.g. ".text.vdso", ".data.vdso") and manually put that section into page-aligned.

```linker
.text {
  . = ALIGN(0x1000);
  PROVIDE(_text_vdso_start = .);
  *(.text.vdso)
  PROVIDE(_text_vdso_end = .);
}
```

```rust
#[link_section = ".text.vdso"]
pub extern "C" fn kernel_provided_symbol() -> usize {
  42
}
```