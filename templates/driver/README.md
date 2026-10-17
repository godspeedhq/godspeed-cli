# {{name}}

A Godspeed driver, created by `godspeed new driver {{name}}` against GodspeedOS {{tag}}.

## Files

- `src/main.rs` - the driver. It starts at `{{entry}}`, takes its device's register window (`ctx.mmio()`), and
  drives it. It is started at boot, like a service, and restarted if it dies.
- `contracts/{{name}}.toml` - what the driver **requests**: its device, by PCI class code and register BAR
  (`hw_pci_class`, `hw_pci_bar`), never an address. It grants nothing; the operator approves the device at
  `godspeed install`, and the OS finds which device that is.
- `godspeed.toml` - what kind of project this is, for the `godspeed` CLI.

## What a driver package is given, today

Its device's registers, and nothing else: no DMA arena (a device that writes memory by itself is not granted to
a package yet) and no interrupt (it polls). A class a GodspeedOS driver already handles is refused, so a package
never takes a device from the OS.

## Run

The template drives QEMU's `edu` device, so QEMU must have one: `-device edu`. Then:

    godspeed fetch <publisher>/{{name}}
    godspeed install <publisher>/{{name}}

and the log says `{{name}}: device id 0x010000ed, alive true, 5! = 120`.
