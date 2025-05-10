# rawpsd

rawpsd is a library that handles loading PSD data into a list of minimally-processed in-memory structs. It does not have any opinions about what features PSD files should or do use, or how to interpret those features. Compressed data is decompressed, and some redundant pieces of data like ascii and unicode names stored together are only returned once instead of twice, but aside from things like that, rawpsd is minimally opinionated and tries to just tell you what the PSD file itself says. For example, strings are left as strings instead of being transformed into enums.

rawpsd draws a compatibility support line at Photoshop CS6, the last non-subscription version of Photoshop. Features only supported by newer versions are unlikely to be supported.

rawpsd's docs do not document the entire PSD format, not even its capabilities. You will need to occasionally reference <https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/> and manually poke at PSD files in a hex editor to take full advantage of rawpsd.

## License

CC0, and I have no patents on anything here.
