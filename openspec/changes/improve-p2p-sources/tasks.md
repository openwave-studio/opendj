# Tasks

## 1. Soulseek add-on

- [ ] 1.1 Readiness: estimated wait from queue and speed, measured speed over advertised, recent failures; results ranked by it; tests
- [ ] 1.2 Unwanted copies (karaoke, sped up, nightcore, 8D, cover…) rank last unless asked for; remixes stay; tests
- [ ] 1.3 Sign-out pause: failed transfers while slskd is signed out are held and asked for again from the same user; test with a mock slskd

## 2. Fake lossless

- [x] 2.1 Spectrum cutoff measure in the engine, with tests on synthetic MP3-like and full-band audio
- [ ] 2.2 Check finished lossless downloads and lossless tracks on Analyze; Lossy tag; amber chip on Downloads
- [ ] 2.3 Health list of fake-lossless tracks with Find another copy

## 3. Showing it

- [ ] 3.1 `Found` readiness line; Ready column on Soulseek results and folders (songs found vs expected)
- [ ] 3.2 "~N min wait" on Downloads; sign-out banner
- [ ] 3.3 Other versions group, closed by default

## 4. Music servers

- [ ] 4.1 Similar artists' songs (`getSimilarSongs2`)
