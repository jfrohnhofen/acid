/* Raspberry Pi Pico (RP2040) with 2MB Flash */
MEMORY
{
  /* 256 bytes for second stage bootloader */
  BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
  /* 2044K - 0x100 for firmware; top 4K (at 0x101FF000) reserved for persistent config */
  FLASH : ORIGIN = 0x10000100, LENGTH = 2044K - 0x100
  RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

SECTIONS {
  .boot2 ORIGIN(BOOT2) :
  {
    KEEP(*(.boot2));
  } > BOOT2
} INSERT BEFORE .text;
