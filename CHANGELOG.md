# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## [0.1.1](https://github.com/MRDGH2821/refpinner/compare/v0.1.0..v0.1.1) - 2026-11-18

### Bug Fixes

- **(github)** isolate rust CI test credentials - ([8f7b9f7](https://github.com/MRDGH2821/refpinner/commit/8f7b9f714448614b5b4901a2422d146ff24f8e2e)) - MRDGH2821
- **(github)** harden release workflow - ([493ffca](https://github.com/MRDGH2821/refpinner/commit/493ffca775ebcd7455ac2393c6fa497145ad1fc4)) - MRDGH2821
- **(jscpd)** clear copy-paste clones in src and skip tests - ([d130b86](https://github.com/MRDGH2821/refpinner/commit/d130b864495b43a8262802c053500095a3cdb521)) - MRDGH2821
- isolate credentials CI flakes on Ubuntu and Windows - ([cd789ff](https://github.com/MRDGH2821/refpinner/commit/cd789ff921465c7a2e3dfa209fb376faa6e45d5b)) - MRDGH2821
- join PATH with platform separator in credentials test - ([43b749b](https://github.com/MRDGH2821/refpinner/commit/43b749bc4cf9203971c96ac1ed3721e0b7569c05)) - MRDGH2821

### Miscellaneous Chores

- **(copier)** update template - ([55d84be](https://github.com/MRDGH2821/refpinner/commit/55d84bece9cebe2ff20fd6bde1aab2d8971584ab)) - MRDGH2821
- merge pull request #21 from MRDGH2821/chore/ci-release-fix - ([b8b2e72](https://github.com/MRDGH2821/refpinner/commit/b8b2e72999d467de69856e45cce2ecba67a8b0f4)) - Mihir Rabade
- merge pull request #24 from MRDGH2821/MRDGH2821/fix/rust-ci-tests - ([ebcf040](https://github.com/MRDGH2821/refpinner/commit/ebcf040e04d1013c7420ebaab8905cfd826dc166)) - Mihir Rabade

### Build

- **(mise)** add cargo-dist - ([ce6b54c](https://github.com/MRDGH2821/refpinner/commit/ce6b54c9a19bb90c9031dda071215583e0130430)) - MRDGH2821
- **(mise)** add cargo-xwin and clang for Windows cross builds - ([8b31089](https://github.com/MRDGH2821/refpinner/commit/8b31089b2255af95c98dfac8c77d58f94bad76c3)) - MRDGH2821
- **(mise)** update versions & lockfiles - ([b4327d5](https://github.com/MRDGH2821/refpinner/commit/b4327d5ffed8597b8e72629d5a8b64b61139a321)) - MRDGH2821

### Ci

- **(github)** enable dist PR artifact uploads - ([2523244](https://github.com/MRDGH2821/refpinner/commit/2523244ba0e4087eff8fa932ece7f689e35bdd67)) - MRDGH2821
- **(github)** build Win ARM on windows-11-arm - ([6451df1](https://github.com/MRDGH2821/refpinner/commit/6451df1429ce50b4b42f77b81efee4d0927536c9)) - MRDGH2821
- update versions - ([d5dbbf6](https://github.com/MRDGH2821/refpinner/commit/d5dbbf65ecfc8a5da5631aa16503eff9fef19116)) - MRDGH2821

---

## [0.1.0] - 2026-11-15

### Bug Fixes

- skip indentation check for LICENSE - ([f09e6ac](https://github.com/MRDGH2821/refpinner/commit/f09e6ac947398afba510ea3f48361e9e43c3811b)) - MRDGH2821
- accept case-insensitive registry bearer challenges - ([0e00bbf](https://github.com/MRDGH2821/refpinner/commit/0e00bbf9487cb0b86f99370cbc76823a67f180cd)) - MRDGH2821
- preserve occupied temporary files - ([7c46e2d](https://github.com/MRDGH2821/refpinner/commit/7c46e2d81623d694b2d5454ad9db9d766349ae86)) - MRDGH2821
- require explicit follow for unannotated pins - ([3ccbe35](https://github.com/MRDGH2821/refpinner/commit/3ccbe35c6359956f62895cb19478f840c86b895f)) - MRDGH2821

### Documentation

- **(mise)** record pinning terms and release research - ([9484076](https://github.com/MRDGH2821/refpinner/commit/94840768d210d9e6f92bd30865bc0f67199ecc3e)) - MRDGH2821
- **(mise)** record project discovery decision - ([218c5f6](https://github.com/MRDGH2821/refpinner/commit/218c5f69874cf77ea5442a5d2c4754fcaac9396b)) - MRDGH2821
- **(mise)** document configuration filename research - ([186cea5](https://github.com/MRDGH2821/refpinner/commit/186cea5082ff24e530f9c84468adc30cbb8eb9d4)) - MRDGH2821
- add handoff for refpinner implementation - ([5694228](https://github.com/MRDGH2821/refpinner/commit/569422864a2dc08202a2276505c73b4c0955d5d7)) - MRDGH2821
- record implementation recovery - ([ac773de](https://github.com/MRDGH2821/refpinner/commit/ac773def93bcd97cb46efc1c847f1ce9cc764b0f)) - MRDGH2821
- describe image pin architecture - ([d1a3fce](https://github.com/MRDGH2821/refpinner/commit/d1a3fce68173e75c07b6af69a29ab0e318a06dad)) - MRDGH2821
- note that image updates are available - ([ecf3015](https://github.com/MRDGH2821/refpinner/commit/ecf30157c48fe903c096b6a0cc4f8d9676da1c22)) - MRDGH2821

### Features

- **(github)** implement GitLab project include pinning with default-ref insertion - ([28df512](https://github.com/MRDGH2821/refpinner/commit/28df51264607906c30660150eadf6c751a15e6d7)) - MRDGH2821
- **(github)** add GitLab project include support - ([e37a8f5](https://github.com/MRDGH2821/refpinner/commit/e37a8f52e80320c5e23223a9ad61d172d2c519a4)) - MRDGH2821
- **(github)** add Forgejo action source settings and shared resolver - ([3b71207](https://github.com/MRDGH2821/refpinner/commit/3b7120799b8e138d0d8801127b4e110757a01699)) - MRDGH2821
- **(mise)** add local TOML configuration with explicit inheritance - ([3c533e2](https://github.com/MRDGH2821/refpinner/commit/3c533e26f9411e9469d07374b124ab043b32b150)) - MRDGH2821
- **(version)** select prereleases and release candidates - ([e50e6bf](https://github.com/MRDGH2821/refpinner/commit/e50e6bf912593255303bd79a4df39ec76b568929)) - MRDGH2821
- **(version)** add prerelease and release candidate selection - ([4900263](https://github.com/MRDGH2821/refpinner/commit/4900263ccb3994e6f3084c49a67ad64eeab0c8a2)) - MRDGH2821
- lint GitHub Actions references offline - ([c5521f3](https://github.com/MRDGH2821/refpinner/commit/c5521f355e7e131f20531061f5351b16526e17f3)) - MRDGH2821
- preview pins for GitHub action references - ([beac20c](https://github.com/MRDGH2821/refpinner/commit/beac20cde466b53fc28bdf858c9f325f507336f3)) - MRDGH2821
- lint reusable workflow references offline - ([68cbc88](https://github.com/MRDGH2821/refpinner/commit/68cbc882bd15eb8971734dc62866ece33800357e)) - MRDGH2821
- lint composite action references offline - ([504cf6b](https://github.com/MRDGH2821/refpinner/commit/504cf6bb317721c0a3156ee29f74cde7d8738efc)) - MRDGH2821
- warn on dynamic references, anchors and aliases - ([6b638d8](https://github.com/MRDGH2821/refpinner/commit/6b638d86ced559960c26d8db6f0e8bfe8dfae549)) - MRDGH2821
- report warnings in pin preview and skip files with YAML aliases - ([ae59c24](https://github.com/MRDGH2821/refpinner/commit/ae59c24358b1ff43fbfcf95bbc9d4957c76ad282)) - MRDGH2821
- apply reviewed pins with numbered backups - ([327af9b](https://github.com/MRDGH2821/refpinner/commit/327af9b3e8ee2f7a27e38fec1d64366c5ca5e2c8)) - MRDGH2821
- discover projects and select workflow files - ([f4656c7](https://github.com/MRDGH2821/refpinner/commit/f4656c758d438aab5c9f471f1603570cf9f5bc13)) - MRDGH2821
- update pinned Git references within version limits - ([9222e58](https://github.com/MRDGH2821/refpinner/commit/9222e58486c1a72f11aba938e81a4d801bb3e118)) - MRDGH2821
- lint and pin workflow container images - ([a3931c9](https://github.com/MRDGH2821/refpinner/commit/a3931c9708c0a5e3ca13c9fcd5113f842e830745)) - MRDGH2821
- integrate GitLab project includes - ([129788e](https://github.com/MRDGH2821/refpinner/commit/129788ec0445c541d43264942b0bf5bcb7fef7d7)) - MRDGH2821
- merge config inheritance into integration - ([4ee5a90](https://github.com/MRDGH2821/refpinner/commit/4ee5a900d75fc958a73157d53adec038a0422985)) - MRDGH2821
- update image versions and compound tags - ([70deeac](https://github.com/MRDGH2821/refpinner/commit/70deeac1368b4a321cb71092bd354c2b1bc932a0)) - MRDGH2821
- merge image version updates into integration - ([e6ea596](https://github.com/MRDGH2821/refpinner/commit/e6ea59669af4f76d3d99e6ab07e6ce36fa062315)) - MRDGH2821
- authenticate private forge and registry dependencies - ([dd0ad3d](https://github.com/MRDGH2821/refpinner/commit/dd0ad3da017f5a3d5f604f9e58be92fbdd3658b5)) - MRDGH2821
- merge private dependency authentication into integration - ([3c31a75](https://github.com/MRDGH2821/refpinner/commit/3c31a75c6e8519dc4e380f06ebe132e6cccf2ca0)) - MRDGH2821
- add online lint verification and JSON output - ([c5eced9](https://github.com/MRDGH2821/refpinner/commit/c5eced97ec6c89ec1e685065997c5a1c5af0c8b2)) - MRDGH2821
- merge online lint verification and JSON output into integration - ([505eef7](https://github.com/MRDGH2821/refpinner/commit/505eef7917b848440df84a300a5cfa796ecaa9af)) - MRDGH2821

### Miscellaneous Chores

- **(copier)** initialise template - ([813f8ca](https://github.com/MRDGH2821/refpinner/commit/813f8ca1d1af5f513110c1c13179f7d25cd0ea9e)) - MRDGH2821
- **(copier)** update template - ([c2e9834](https://github.com/MRDGH2821/refpinner/commit/c2e9834bd3497840c1f620afcb227d3c6e7dfccc)) - MRDGH2821
- **(cspell)** add Rust tooling words - ([781d1c2](https://github.com/MRDGH2821/refpinner/commit/781d1c25607c5e728c116a60f0cb979514e94d93)) - MRDGH2821
- **(cspell)** add HTTP client and GitHub API words - ([b2d3cff](https://github.com/MRDGH2821/refpinner/commit/b2d3cffbf7e6bff85e10b3939dd9b1e95d9b0c16)) - MRDGH2821
- **(cspell)** add rignore for .v8rignore - ([e1fa19e](https://github.com/MRDGH2821/refpinner/commit/e1fa19e7fefba6ec6f8527697afebf03911d3ffb)) - MRDGH2821
- **(cspell)** update words - ([b44af32](https://github.com/MRDGH2821/refpinner/commit/b44af32a920d81d51c4098c6f953cb0edd8e856f)) - MRDGH2821
- **(github)** configure issue tracker for agent skills - ([c1a3677](https://github.com/MRDGH2821/refpinner/commit/c1a3677cc90abfb24a7d80dade857b30ba55126f)) - MRDGH2821
- **(mise)** configure agent skills workflow - ([320367e](https://github.com/MRDGH2821/refpinner/commit/320367e9ab5f7b760254b684f71c0a28810bfeae)) - MRDGH2821
- merge pull request #16 from MRDGH2821/MRDGH2821/chore/configure-agent-skills - ([5a2aeaa](https://github.com/MRDGH2821/refpinner/commit/5a2aeaa20496ba40e2edc333889ea0e5ce49ae50)) - Mihir Rabade
- update placeholders - ([aed9b8d](https://github.com/MRDGH2821/refpinner/commit/aed9b8de23da9f040d2b1080516f7298f50e5eeb)) - MRDGH2821
- merge MRDGH2821/feat/refpinner into MRDGH2821/feat/preview-action-pins - ([d27d282](https://github.com/MRDGH2821/refpinner/commit/d27d282fa3e27b7c6a6842b6bfdf06ee7375536c)) - MRDGH2821
- merge MRDGH2821/feat/refpinner into discover-projects - ([2adebfa](https://github.com/MRDGH2821/refpinner/commit/2adebfa42696edb8bd5ee484f6ae7c169000a21c)) - MRDGH2821
- merge MRDGH2821/feat/refpinner into MRDGH2821/feat/update-git-refs - ([040d43f](https://github.com/MRDGH2821/refpinner/commit/040d43fbc9639f17b94d09aed55f062d800d5aa8)) - MRDGH2821
- merge MRDGH2821/feat/refpinner into MRDGH2821/feat/update-git-refs - ([83fb034](https://github.com/MRDGH2821/refpinner/commit/83fb0342c07a56cec1ff655409f4ef160bbc54ef)) - MRDGH2821
- merge shared pin and update logic into image pins - ([5184028](https://github.com/MRDGH2821/refpinner/commit/5184028ee9d871e2bc776365b6e9507468bb1c1d)) - MRDGH2821
- merge latest integration into image pins - ([666eb31](https://github.com/MRDGH2821/refpinner/commit/666eb31041417aec75779a4363feaf8c4bd0fd2c)) - MRDGH2821
- recover image and prerelease integration - ([a377437](https://github.com/MRDGH2821/refpinner/commit/a377437ca34e06c5d189fbe17a177b9ff4725480)) - MRDGH2821
- merge pull request #18 from MRDGH2821/MRDGH2821/feat/refpinner - ([063a833](https://github.com/MRDGH2821/refpinner/commit/063a833009cd900646f9c3fea845249a0be3e1c2)) - Mihir Rabade
- merge branch 'main' into dev - ([64cad1b](https://github.com/MRDGH2821/refpinner/commit/64cad1b091dd8bbf90784384811c257eb1db3cac)) - MRDGH2821
- merge pull request #19 from MRDGH2821/MRDGH2821/feat/cargo-dist-packslip - ([f92864d](https://github.com/MRDGH2821/refpinner/commit/f92864d60f592b3a019af5abecb30f6cd09bc8e9)) - Mihir Rabade
- merge pull request #20 from MRDGH2821/dev - ([b03dc9e](https://github.com/MRDGH2821/refpinner/commit/b03dc9ef98ca4a33403c8b9736aea53d7e174855)) - Mihir Rabade
- add cargo version bumper - ([6455925](https://github.com/MRDGH2821/refpinner/commit/64559250b63a50e6bc8e77a6f62a5f60fe3eaceb)) - MRDGH2821

### Refactoring

- share the preview and write loop between commands - ([1b09aaf](https://github.com/MRDGH2821/refpinner/commit/1b09aaf021ab6fce0d8f2e85891024397946c22a)) - MRDGH2821

### Style

- format files - ([01e6c6f](https://github.com/MRDGH2821/refpinner/commit/01e6c6fbc62e37ea3bcd0deb8a2be0dff0a38391)) - MRDGH2821

### Tests

- share the mock forge between CLI test files - ([34d06e5](https://github.com/MRDGH2821/refpinner/commit/34d06e539a813f33346286eb45ea85bd193cb52b)) - MRDGH2821
- cover empty pin preview - ([c5537bc](https://github.com/MRDGH2821/refpinner/commit/c5537bc910c7e0bb5c3b391e5e803033dec4c611)) - MRDGH2821
- verify image pins use shared safe writes - ([077ac6d](https://github.com/MRDGH2821/refpinner/commit/077ac6df50523d4f68708794c5d468634168f446)) - MRDGH2821

### Build

- **(hk)** add Rust check group - ([53515d9](https://github.com/MRDGH2821/refpinner/commit/53515d93fd74f079f9a3bb7eb1ac4e91695a0058)) - MRDGH2821
- **(mise)** add Rust toolchain with Mr Boxington - ([5a8deea](https://github.com/MRDGH2821/refpinner/commit/5a8deeac9899e3580703fa107d6f9f5e6afc6dee)) - MRDGH2821
- **(mise)** bump versions - ([70ecaee](https://github.com/MRDGH2821/refpinner/commit/70ecaeebe8f5612a3736891bf2369c10f044c979)) - MRDGH2821
- **(mise)** update versions - ([63b2d65](https://github.com/MRDGH2821/refpinner/commit/63b2d65f40c867f5f3572628a308efd1db70127b)) - MRDGH2821
- install dependencies - ([dc0254f](https://github.com/MRDGH2821/refpinner/commit/dc0254f9ca46900f6453141ac303ddf75b734ccd)) - MRDGH2821
- update lock files - ([6720f88](https://github.com/MRDGH2821/refpinner/commit/6720f88011c2a7fb8151170e6c93c10853043b68)) - MRDGH2821
- merge MRDGH2821/feat/pin-images into integration - ([88a46b2](https://github.com/MRDGH2821/refpinner/commit/88a46b2c67e178e6e50f07367adaf6be377807d8)) - MRDGH2821

### Ci

- **(github)** test Rust CLI on Linux, macOS and Windows - ([2d2bfaf](https://github.com/MRDGH2821/refpinner/commit/2d2bfafa9bbdc3d494e9cdd3ea3947429d49faac)) - MRDGH2821
- **(github)** add cargo-dist and packslip release pipeline - ([05aba37](https://github.com/MRDGH2821/refpinner/commit/05aba375c14a944a90eda036fdbcea25cce4ed1e)) - MRDGH2821

<!-- generated by git-cliff -->
