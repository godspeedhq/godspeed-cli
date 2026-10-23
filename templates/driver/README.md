# {{name}}

A Godspeed driver, created by `godspeed new driver {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the driver. It starts at `{{entry}}` and uses everything a driver is given, through `gs::driver`:
  the device's register window (`registers`), its interrupt (`irq`), and a DMA arena laid out by name (`arena`).
  It is started at boot, like a service, and restarted if it dies.
- `contracts/{{name}}.toml` - what the driver **requests**: its device, by PCI class code and register BAR
  (`hw_pci_class`, `hw_pci_bar`), never an address; its interrupt (`hw_pci_irq`); and a DMA arena by size
  (`hw_dma`). It grants nothing. The operator approves the device at `godspeed install`, and DMA memory by a
  question of its own; the OS finds which device it is.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.

## What a driver package is given

- **Its device's registers**, found by class.
- **Its interrupt**, when the contract asks: the OS chooses the vector and programs the device with it.
- **A DMA arena**, when the contract asks and the operator agrees: memory the device reads and writes by itself,
  at most 2 MiB. Where no IOMMU confines the device - no Raspberry Pi or VisionFive has one - a driver with an
  arena can read and write all of memory, which is why it is asked about on its own.
- **A device a GodspeedOS driver already drives**: installing this one stops that driver for the device, and
  uninstalling it starts that driver again.
- **A USB device**, instead of a PCI one: name it by `hw_usb = "vvvv:pppp"` in place of the `hw_pci_*` keys, and
  reach it through its host with `gs::driver::bus::UsbDevice` (its identity, control transfers, bulk OUT, bulk
  IN). It is started while the device is attached. The host binds a vendor-specific device (interface class
  0xFF), one at a time; on the Raspberry Pi 2 it must be high-speed.

The interrupt, the arena, a device taken over and a USB device need the GodspeedOS release after v0.30.0.

## Run

The template drives QEMU's `edu` device, so QEMU must have one: `-device edu`. `edu` reaches only the first
256 MiB of memory by DMA unless told otherwise, so on a machine with more, use
`-device edu,dma_mask=0xffffffffffffffff`; the driver says so if it needs it. Then:

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}

and the log says `{{name}}: device id 0x010000ed, alive true, 5! = 120 by interrupt, 64 bytes through DMA and back intact`.
