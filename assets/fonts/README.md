# Bundled fonts

Postman GPUI embeds these variable TrueType fonts in the executable so text rendering does not
depend on platform-installed families:

| Family | Source file | SHA-256 |
| --- | --- | --- |
| Space Grotesk | `ofl/spacegrotesk/SpaceGrotesk[wght].ttf` | `acad6de1fc93436f5c0f1f4137751ef04f1aea3063e7036535970ffcfbd79f72` |
| Manrope | `ofl/manrope/Manrope[wght].ttf` | `d0639be45d0af36e798172419d7bd173c4bd4f29e2b76cbb69db1d11bf8b0a40` |
| JetBrains Mono | `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | `48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda` |

The files came from the [Google Fonts repository](https://github.com/google/fonts) at commit
`6a003b5eb672dc8bf5bff5937cf5863f8b175445`. Each family is distributed under the SIL Open Font
License 1.1 included beside its font file.
