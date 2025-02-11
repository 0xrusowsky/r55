/* Pass this linker script alongside with riscv-rt's link.x */

MEMORY
{
  CALL_DATA : ORIGIN = 0x80000000, LENGTH = 1M
  STACK : ORIGIN = 0x80100000, LENGTH = 2M
  REST_OF_RAM : ORIGIN = 0x80300000, LENGTH = 1021M
}

SECTIONS
{
  /DISCARD/ : {
    *(.eh_frame)
    *(.eh_frame_hdr)
    *(.eh_frame.*)
  }
}

REGION_ALIAS("REGION_TEXT", REST_OF_RAM);
REGION_ALIAS("REGION_RODATA", REST_OF_RAM);
REGION_ALIAS("REGION_DATA", REST_OF_RAM);
REGION_ALIAS("REGION_BSS", REST_OF_RAM);
REGION_ALIAS("REGION_HEAP", REST_OF_RAM);
REGION_ALIAS("REGION_STACK", STACK);

INCLUDE link.x
