## Feb 01, 2023
- PoC:
    + Event loop
    + Framing clients
    + Button/Key handling
    + Display stack changing
- Todo:
    + Layout policy (tiling, floating, tabbed)
    + Configuration
    + ICCCM / EWMH 

## Mar 18, 2023
Allow user to configure the WM at runtime using a YAML file.

The configuration file will be structured as in the [example configuration file](../examples/config.yaml).

Currently, only default configuration are supported. The file must be placed in `$XDG_CONFIG_HOME/lazywm`.
