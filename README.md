# youarehere

A minimalist shell prompt showing the user, hostname, directory, and git branch.
A tiny [starship](https://github.com/starship/starship).

## Install

```bash
# Straight from GitHub:
cargo install --git https://github.com/eljpsm/youarehere

# With Nix:
nix run github:eljpsm/youarehere

# From a clone (installs to ~/.cargo/bin):
make install
```

## Usage

Add to your shell config:

```bash
# .bashrc
eval "$(youarehere init bash)"

# .zshrc
eval "$(youarehere init zsh)"

# config.fish
youarehere init fish | source
```

The prompt is one line:

```text
eljpsm@laptop ~/src/github/eljpsm/youarehere (main v0.1.0)
```

Git is optional. Without it, exact tags are omitted and branches still show.

## Development

Enter `nix develop`, then use the Makefile:

```bash
make test
make bench
```

## Acknowledgements

youarehere is inspired by:

- [starship](https://github.com/starship/starship)

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
