# Bundled fonts

Postman GPUI embeds these TrueType fonts in the executable so text rendering does not depend on
platform-installed families. Inter uses explicit static weights for consistent native text-backend
selection, while JetBrains Mono remains a variable font:

| Family | Source file | SHA-256 |
| --- | --- | --- |
| Inter Regular | `extras/ttf/Inter-Regular.ttf` | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| Inter Medium | `extras/ttf/Inter-Medium.ttf` | `97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872` |
| Inter SemiBold | `extras/ttf/Inter-SemiBold.ttf` | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |
| Inter Bold | `extras/ttf/Inter-Bold.ttf` | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |
| JetBrains Mono | `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | `48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda` |

Inter came from the official [Inter v4.1 release](https://github.com/rsms/inter/releases/tag/v4.1).
JetBrains Mono came from the [Google Fonts repository](https://github.com/google/fonts) at commit
`6a003b5eb672dc8bf5bff5937cf5863f8b175445`. Both families are distributed under the SIL Open Font
License 1.1 included beside their font files.
