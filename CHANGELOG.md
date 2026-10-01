## [1.3.0](https://github.com/MSpiechowicz/useful-timer/compare/v1.2.1...v1.3.0) (2026-09-30)

### Features

* add polished Clockwork Bloom timer ([805113e](https://github.com/MSpiechowicz/useful-timer/commit/805113e78064a2d5619231b9fa7790f33237aad1))

## [1.2.1](https://github.com/MSpiechowicz/useful-timer/compare/v1.2.0...v1.2.1) (2026-09-30)

### Bug Fixes

* smooth timer seams and brighten wand finale ([15a0983](https://github.com/MSpiechowicz/useful-timer/commit/15a098333db4db5baadaef4a76ed9b7cccb49806))

## [1.2.0](https://github.com/MSpiechowicz/useful-timer/compare/v1.1.0...v1.2.0) (2026-09-30)

### Features

* add cinematic timer styles and illustrated crescent wand ([89415e8](https://github.com/MSpiechowicz/useful-timer/commit/89415e80727c9152cbfd4341b8b4ea18e1f8f808))

## Unreleased

### Features

* Add Code Rain, Machine Core, and four-star Dragon Orb timer artwork with distinct finite completion effects and synthesized sounds.
* Expand the adaptive artwork picker to eight styles and add a saved per-timer Reduced motion option.
* Keep new-style animation poses continuous across pause/resume and remaining-time adjustments.
* Add illustrated Crescent Wand with an engraved crescent, compact upward-flaring ribbon folds and a three-bead center, round pearlescent crystal on a gold pedestal, falling sparkles, crystal-light/star-shower completion, and a synthesized chime.
* Add Clockwork Bloom with articulated porcelain petals, gold edges and stamens, physical hinges, a tiered hub and plinth, a remaining-time bar, and a synthesized latch-and-chime completion sound.
* Display and edit sound volume as a percentage.
* Add configured-duration minute adjustments and compact presets for 30 seconds, 1/3/5/10/15/30 minutes, and 1/2 hours.
* Save a custom default duration for new timers across restarts without changing existing timers.
* Separate settings sections with dividers and use outlined secondary actions with additional spacing.
* Simplify the README for everyday users and add screenshots of all eight timer styles and the countdown workspace.

### Bug Fixes

* Render Clockwork Bloom with per-pixel depth testing, antialiased metal hardware and petals, finer plinth geometry, and supersampled pixel coverage; keep solid hardware lighting continuous through grazing angles.
* Make Bloom's completion unfold monotonically into a stationary final pose, without reversal or oscillation, and keep its caption fixed above the entire animation with a small margin.
* Keep Hours, Minutes, and Seconds inputs fixed-width while editing long values.
* Refine Machine Core sizing and spacing, remove the pale lens reflection and duplicate progress bar, and close its iris fully dark.
* Keep Machine Core's white arcs rotating steadily and refine Dragon Orb's layered glass highlights.
* Refine Crescent Wand with smaller, softly shaded ribbon folds below a raised crescent, a continuous pink neck joining the head to the medallion, and no decorative arcs above or below its jewels.
* Seat Crescent Wand's crystal on a small gold pedestal behind the crescent's inner rim rather than stacked rings; keep the glow, glints, and star shower anchored to the crystal.
* Reduce Crescent Wand's crystal and its lighting by 25% without moving its seat, remove the bottom-cap white line, and contain and soften the crescent's reflections.
* Separate Crescent Wand's spiral engravings from its rim highlights and continue the inner highlight smoothly along the crescent.
* Finish Crescent Wand's lower end cap in gold instead of pink, matching the surrounding gold collar.
* Center Rocket's upper seam with mirrored endpoints and control points.
* Smooth Bomb's upper and lower countdown seams with scale-correct edge antialiasing.
* Brighten Crescent Wand's finale with a white-gold crystal starburst, smoothly feathered bloom, and thin rotating light trails with outward-moving highlights; preserve idle artwork and Reduced motion.

## [1.1.0](https://github.com/MSpiechowicz/useful-timer/compare/v1.0.0...v1.1.0) (2026-09-30)

### Features

* add startup updates with checksum result toasts ([2ee6c2e](https://github.com/MSpiechowicz/useful-timer/commit/2ee6c2e7629d59cabd0e9fba1bd781c54af3175c))

## 1.0.0 (2026-09-30)

### Features

* add curl installers for desktop releases ([0a60a07](https://github.com/MSpiechowicz/useful-timer/commit/0a60a07a172f22dd71c423f1516a104cc1e69205))
* add persistent themes and separate widget and sound settings ([9759500](https://github.com/MSpiechowicz/useful-timer/commit/97595005bd0bd72c407dfac6112b2bbe5f1c7d85))
* automate releases and place new widgets at bottom left ([4d0d88b](https://github.com/MSpiechowicz/useful-timer/commit/4d0d88bb539d11dea9d05efd3ee9be3af7fd43a7))

### Bug Fixes

* correct timer artwork layering and seam alignment ([13058bc](https://github.com/MSpiechowicz/useful-timer/commit/13058bcaae54939dd9eb559b4d4a191496a11a57))
