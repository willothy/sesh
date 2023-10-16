# Changelog

## [0.1.11](https://github.com/willothy/sesh/compare/v0.1.11...v0.1.11) (2023-10-16)


### ⚠ BREAKING CHANGES

* display program instead of socket in list
* make no args launch new session

### Features

* `PtyBuilder` utility ([475ee7a](https://github.com/willothy/sesh/commit/475ee7a74c954730a47c269a7e1501f627acf3bb))
* accept start cmd args without subcommand ([fdcce09](https://github.com/willothy/sesh/commit/fdcce09f1db7ae763bc4e6b0cd6de4c8ee4ebba5))
* add JSON output option for list subcommand ([4a08943](https://github.com/willothy/sesh/commit/4a0894309308700887e459d6745d635b274a090b))
* allow env vars in PTY ([cbebd67](https://github.com/willothy/sesh/commit/cbebd677808197cce1b778ae9ff6cd6c9a5252b6))
* attach and detach ([29cf490](https://github.com/willothy/sesh/commit/29cf49015531eda7120313905a8645ec35c2f372))
* auto-start and exit seshd ([17e4705](https://github.com/willothy/sesh/commit/17e47051fa788b04f64da513580526b150289fa9))
* change detach to alt-\ ([3c565c9](https://github.com/willothy/sesh/commit/3c565c9fb95a43cc6b4a6a96a6175ba5fb5242c9))
* client-&gt;server connection for commands ([30cd804](https://github.com/willothy/sesh/commit/30cd8040a5a263901b294355d222911e92f69e52))
* don't require tty for client ([ec34e7a](https://github.com/willothy/sesh/commit/ec34e7a5fb47ac17301b2ea16e338fe2256b19ad))
* even nicer client output ([37716a8](https://github.com/willothy/sesh/commit/37716a85de9bc6fd7a9b564eda37b5e03bcb6911))
* fuzzy select sessions ([22484b7](https://github.com/willothy/sesh/commit/22484b7baf10ae6fe3978870f4364d525f152584))
* graceful program exit ([cd1b362](https://github.com/willothy/sesh/commit/cd1b36280e5105db68669e7a6132106db9cedbc8))
* handle child process exit ([0e05162](https://github.com/willothy/sesh/commit/0e051624ac9348fe22ab7c8ebe007f590ce0d745))
* inherit client cwd in new sessions ([f95b3a6](https://github.com/willothy/sesh/commit/f95b3a699f04d28d78f7af8af09f0e280a545660))
* inherit env from parent shell / process ([8429f63](https://github.com/willothy/sesh/commit/8429f6305c2145798902378bcb406314393d98cf))
* initial commit ([d6e3cdf](https://github.com/willothy/sesh/commit/d6e3cdf9d96eb29f1e6b94f824223387b831df4e))
* it works! (kinda) ([47e475c](https://github.com/willothy/sesh/commit/47e475c3d04a478d9937355358d889be7429e0f6))
* nicer CLI output ([a71984d](https://github.com/willothy/sesh/commit/a71984d7294db1c5ce6d3d8e3230e0d1b89f2b21))
* nicer exit handling with mpsc ([2173119](https://github.com/willothy/sesh/commit/2173119ce790ceb027ceef845af48a5a8e84e1e8))
* nicer list output ([9f891f1](https://github.com/willothy/sesh/commit/9f891f15364e5c949897c8d729df05351817aa6e))
* print session info as table ([7574a62](https://github.com/willothy/sesh/commit/7574a62a4cc0390df588cea4a2ed74177839390b))
* remote detach ([a1bbbbb](https://github.com/willothy/sesh/commit/a1bbbbb525c427a8527eb593c94c9cf67a7d8d4e))
* resize hack to restore screen ([7f7a4d3](https://github.com/willothy/sesh/commit/7f7a4d3260472f8748228bc83a53fbb5827de7ef))
* resume last-used session ([9b5f768](https://github.com/willothy/sesh/commit/9b5f768175e586dee1257a21a6704f3393c9f54b))
* **resume:** create new session if nonexistent ([a6bc638](https://github.com/willothy/sesh/commit/a6bc638ed1d96962a514f0cf2614ca0dfc2bc9dd))
* set terminal title to running program name ([2c20511](https://github.com/willothy/sesh/commit/2c2051148ac9a1b2d5341317df57c59cb37d1a00))
* show subprocess pid with `list --info` ([3693044](https://github.com/willothy/sesh/commit/36930442060460b246ea8a03d438ff66837828ac))
* smarter CLI session selection ([8a5c6f4](https://github.com/willothy/sesh/commit/8a5c6f4e2667d2c80b06b8996ff0b3537adfa9da))
* use alternate screen ([0b10129](https://github.com/willothy/sesh/commit/0b101290ddb773682f971d594920739abc7806da))
* use libc::fork to launch server ([0fdb620](https://github.com/willothy/sesh/commit/0fdb6206cf6a36701499472252d62ca4c0f52d95))
* wezterm integration ([1c6a083](https://github.com/willothy/sesh/commit/1c6a083386149294f01ed9f366d080d37174a5b2))


### Bug Fixes

* **ci:** install protobuf in rust workflow ([904d28e](https://github.com/willothy/sesh/commit/904d28efd8c4cc95d9a63212fb6f23d0f8ad3016))
* delete .md ([323c6b9](https://github.com/willothy/sesh/commit/323c6b955ef2b8c4e4b7f01bfa65e1a45336b0e2))
* delete client server socket if it exists ([a6bc638](https://github.com/willothy/sesh/commit/a6bc638ed1d96962a514f0cf2614ca0dfc2bc9dd))
* **docs:** integration section ([5d40d18](https://github.com/willothy/sesh/commit/5d40d1890903edea04617ff8335d562bde5f6674))
* don't output test title ([0639361](https://github.com/willothy/sesh/commit/0639361f8d1d89da7f04551b1929c2d13af8eafa))
* don't use protobuf optional types ([15f54e1](https://github.com/willothy/sesh/commit/15f54e1af94c39fa84878539f74bf269a26a115b))
* lag from delays ([96e9070](https://github.com/willothy/sesh/commit/96e907003fc0fb984ff9f19c85b128ed7db3eec3))
* libc int types for osx ([754fd46](https://github.com/willothy/sesh/commit/754fd4608e491e045a485fba0722948a6bfd92a5))
* libc int types for osx pt2 ([056c944](https://github.com/willothy/sesh/commit/056c944168d724d69e2cddb4634007c86f7c1a3e))
* libc int types for osx pt3 ([037ea25](https://github.com/willothy/sesh/commit/037ea25dadc770a5ad7fd1532b4c20b85e13d948))
* reset terminal colors after list ([ba0ae84](https://github.com/willothy/sesh/commit/ba0ae84f0ad2d90ba0aa2c17f80be761b49a901a))
* tui size management ([ec34e7a](https://github.com/willothy/sesh/commit/ec34e7a5fb47ac17301b2ea16e338fe2256b19ad))
* update .gitignore ([2a83e8c](https://github.com/willothy/sesh/commit/2a83e8c66cb40a4216c514f02b7a6fde9592f9b2))
* **wezterm:** switch to prev window on cancel ([baa62f4](https://github.com/willothy/sesh/commit/baa62f43673cf40e8ab20d91c38bed90a0ce16d1))


### Miscellaneous Chores

* bump versions ([c6f1487](https://github.com/willothy/sesh/commit/c6f1487b47374cb419c041bcec23c112ded70da1))


### Code Refactoring

* display program instead of socket in list ([a63eb24](https://github.com/willothy/sesh/commit/a63eb240f096e23991bbde0d5ff7d389c0988c60))
* make no args launch new session ([0e9f7e8](https://github.com/willothy/sesh/commit/0e9f7e8c57ae4c732481d157cac0b5d448d888ff))

## [0.1.11](https://github.com/willothy/sesh/compare/v0.1.10...v0.1.11) (2023-10-16)


### Features

* don't require tty for client ([ec34e7a](https://github.com/willothy/sesh/commit/ec34e7a5fb47ac17301b2ea16e338fe2256b19ad))


### Bug Fixes

* tui size management ([ec34e7a](https://github.com/willothy/sesh/commit/ec34e7a5fb47ac17301b2ea16e338fe2256b19ad))


### Miscellaneous Chores

* bump versions ([c6f1487](https://github.com/willothy/sesh/commit/c6f1487b47374cb419c041bcec23c112ded70da1))
