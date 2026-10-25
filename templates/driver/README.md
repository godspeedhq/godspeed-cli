# {{name}}

A Godspeed driver, created by `godspeed new driver {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the driver. It starts at `{{entry}}` and uses everything a driver is given, through `gs::driver`:
  the device's register window (`registers`), its interrupt (`irq`), and a DMA arena laid out by name (`arena`).
  It is started at boot, like a service (a USB driver: while its device is attached), and restarted if it dies,
  up to five times a boot.
- `contracts/{{name}}.toml` - what the driver **requests**: its device, by PCI class code and register BAR
  (`hw_pci_class`, `hw_pci_bar`), never an address; its interrupt (`hw_pci_irq`); and a DMA arena by size
  (`hw_dma`). It grants nothing. The operator approves the device (`device`) at `godspeed install`, and DMA
  memory by a question of its own (`memory`), asked on every machine; the OS finds which device it is.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.

## What a driver package is given

- **Its device's registers**, found by class.
- **Its interrupt**, when the contract asks: the OS chooses the vector and programs the device with it.
- **A DMA arena**, when the contract asks and the operator agrees: memory the device reads and writes by itself,
  at most 2 MiB. Where no IOMMU confines the device - every Raspberry Pi and the VisionFive, an Intel PC, and any
  PC device the kernel leaves in passthrough - a driver with an arena can read and write all of memory, which is
  why it is asked about on its own.
- **A device a GodspeedOS driver already drives**: installing this one stops that driver for the device, and
  uninstalling it starts that driver again.
- **A USB device**, instead of a PCI one: name it by `hw_usb = "vvvv:pppp"` in place of `hw_pci_class`,
  `hw_pci_bar`, `hw_pci_irq` and `hw_dma` - a USB driver has no interrupt or DMA of its own, they are its host's,
  and a contract that asks for them is refused at install. Reach it through its host with
  `gs::driver::bus::UsbDevice`: who it is and where (`info`, `place`), control transfers (`control_in`,
  `control_out`, and `control_out_once` for a write that must not reach it twice), bulk OUT and bulk IN, and
  `replug` for a device that stopped answering (`xhci` and `ehci` refuse it). It is started while the device is
  attached. Its host binds one vendor-specific device (first interface class 0xFF) that is not a keyboard or a
  mouse, one per host: `xhci` on the PCs, the Pi 4 and the VisionFive; `dwc2` on the Pi 2, high-speed devices
  only; `ehci` on the T630's back sockets, which sends no notice, so call `bulk_in` again rather than waiting to
  be told.

The interrupt, the arena, a device taken over and a USB device need GodspeedOS v0.31.0 or later.

## `gs::driver`, and the SDK

Write the driver on `gs::driver`: it is the way GodspeedOS's own drivers are written, so they are the examples to
read, and it is the part the compatibility promise covers - it only grows, so what you use stays. (GodspeedOS's
drivers are GPL-2.0-only: copying one brings its licence with it, and any SDK call it still makes is outside the
promise below.) You may also call the SDK (`godspeed_sdk`) directly - nothing refuses it, and it gives you nothing
beyond what your contract asked for and the operator granted - but the SDK may change between releases, so a driver
written on it can stop building or working on a later one. If you need something `gs::driver` does not have, that
is a gap in `gs::driver`: report it to GodspeedOS, and it is closed there when a real driver needs it.

## Run

The template drives QEMU's `edu` device, so QEMU must have one: `-device edu`. `edu` reaches only the first
256 MiB of memory by DMA unless told otherwise, so on a machine with more, use
`-device edu,dma_mask=0xffffffffffffffff`; the driver says so if it needs it. Then:

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}

and the log says `{{name}}: device id 0x010000ed, alive true, 5! = 120 by interrupt, 64 bytes through DMA and back intact`.
