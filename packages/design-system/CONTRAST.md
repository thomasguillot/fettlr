# Contrast contract

Every foreground/background pair the components put on screen is declared here and checked by
`pnpm --dir packages/design-system check-contrast` against `src/tokens.css`, in light and dark mode.
A pair that fails fails the build.

The measure is APCA (the Accessible Perceptual Contrast Algorithm, W3 0.1.9 constants), not the
WCAG 2 luminance ratio. APCA reports a lightness contrast `Lc` that is polarity aware, so a pair is
always declared as the foreground on the background, in that order, and the gate takes the absolute
value. The levels follow APCA's own guidance:

- `90` for primary text on the surfaces it rests on: the preferred level for body text.
- `75` for everything else people read: secondary text, status text, labels on filled controls, and
  any text on a hover, selected or tinted surface. This is APCA's minimum for body text.
- `60` for display text: the site's hero word, 44px and up at weight 600. This is APCA's minimum for
  large headline text, and it clears WCAG AA at any size on the way.
- `45` for non-text: control boundaries, focus rings, and the mark's glyph on its tile. This is APCA's minimum for fine-detail pictograms and thin outlines, which is what a
  hairline and a two-pixel ring are.

A pair can also declare a ceiling (`border on bg: 15 max 45`). Hairlines carry one, so that a rule
can never reach the weight of the text it separates; without it, "passes contrast" would quietly
come to mean "as loud as the system allows".

A pair can also carry a dark-mode clause. The two border weights carry one for a physical reason,
and the display blue carries one by ruling. APCA weights contrast by absolute luminance, so holding
a hairline to the same `Lc` in both modes forces a far larger step between rendered values in dark
mode: the dark rule that just clears `Lc 15` sits 64 levels off its surface, where the light rule at
`Lc 19.9` sits 38 off its own. A thin light line on a dark ground also blooms on an emissive
display, so it reads heavier still than the score predicts. Dark hairlines are therefore allowed to
sit lower than the light ones. This is a polarity rule for thin elements, not a waiver: APCA models
text correctly, so text keeps one level in both modes.

The two weights need different instruments, because at the value dark `border` wants, APCA stops
reporting anything at all:

- `border on bg: 15 max 45 (dark step 30..75)` swaps the measure. At `oklch(0.36 …)` on the dark
  grounds APCA returns exactly `Lc 0.0`, the same clamp `surface on bg-subtle` hits, while the rule
  is plainly visible at 39 to 51 rendered levels. Any `Lc` floor above zero would fail a line you can
  see, and any floor at zero would gate nothing, so in dark mode these three pairs are measured as
  the 8-bit step between the two rendered greys, via `oklchToHex`. The range keeps both halves of
  the original intent: a rule that cannot be seen and a rule as heavy as the text it separates are
  both still bugs.
- `border-strong on bg: 45 max 75 (dark min 25)` keeps APCA and lowers the floor. The control
  boundary still scores `Lc 27` in dark, comfortably clear of the clamp, so there is no reason to
  reach for the cruder measure.

`blue-display on bg: 60 (dark min 45)` is a colour ruling, not a polarity one. Blue is the only hue
in the system, and the hero word is the one place it is asked to carry size. On a dark ground
contrast comes from lightness, and lightness is what strips chroma out of blue, so a high `Lc` and a
saturated blue cannot both be had: the word either reads as a pale wash or the floor moves. At
`Lc 60` chroma caps at 0.120 at this hue. At `Lc 50`, where the floor first went, it caps at 0.152.
It now sits at `Lc 45`, which puts the token at `oklch(0.68 0.168 264)` measuring `Lc 45.8`, with 40
percent more chroma than `Lc 60` allows and the dark hue back on the 264 that light uses. It is the
one place in the contract where a level was lowered for a colour rather than for a measure, and it
has been lowered twice.

`45` is not a number invented for this token. It is the level the contract already applies to every
other element of this size, and APCA's own size and weight table asks less than that of text at 44
to 72px and weight 600, which is the range the word renders at. The `60` the level is drawn from is
a body and headline figure, set for text far smaller. The floor stops here even so: APCA is a
luminance measure, blue is the hue it flatters most, and saturated blue text reads worse than its
score suggests.

All three clauses are global to dark mode rather than scoped to one theme, because that is the only
form the gate parses. None of them moves a high contrast or dracula colour: both themes already
clear the step floor and both lowered `Lc` floors, the display blue by a wide margin at `Lc 76.6`
and `Lc 81.0`. The cost is that a future drift down in those two themes would no longer be caught.

Token names are written without the `--color-` prefix.

## How this list is kept honest

The list comes from a sweep of every file in `src/components`, reading each component for the
foreground utilities it applies and every background those foregrounds can land on: the component's
own surface, each of its interactive states, and the containers it is documented to sit in. Menu and
list items contribute their hover, highlight and selected backgrounds as well as their resting one,
because a foreground that only clears its level at rest is not good enough.

Redo the sweep whenever a component gains a colour utility or a new state. The gate cannot discover
pairs on its own; it only proves that the declared ones hold in both modes.

Deliberately outside the gate:

- Nothing: `--color-border` is gated too, between `Lc 15` and `Lc 45` in light mode and between 30
  and 75 rendered levels in dark, because a hairline that cannot be seen and a hairline as heavy as
  text are both bugs. `Lc 15` is APCA's minimum for a non-text element that only has to be
  discernible, which is what a rule between two rows is. `--color-border-strong` is the control
  boundary and sits between `Lc 45` and `Lc 75`, or between `Lc 25` and `Lc 75` in dark.
- `--color-overlay` is translucent by design, so no fixed value can be computed for it. The gate
  rejects any translucent token used in a declared pair rather than measuring it wrongly.
- Disabled controls.
- A fill against the ground it sits on: the primary button, the checked switch track and checkbox, a solid badge, the count pill,
  the accent action button inside a toast, the selected segment of a segmented control. The label on
  the fill is gated instead, since the label
  is what identifies the control. The invariant that makes it safe: a fill token is never used
  for anything thin. The active tab bar, the focus border, the invalid field border and every icon use the
  family's `-text` token, which is gated at `Lc 75` on every ground it can land on.

  `selected` is the one fill whose separation from its ground carries meaning on its own: it is what
  says which segment is chosen, and a readable label does not answer that. Its value is therefore
  picked against `bg-subtle` per theme rather than left free. In the default and dracula themes it is
  that theme's accent pair, so the chosen segment reads as the primary call to action does. It was
  previously a near-`surface` value, which separated from `bg-subtle` at `Lc 0.0` and a step of 7
  rendered levels in default light: the chosen segment was, correctly, reported as almost impossible
  to pick out. The accent fill separates at `Lc 100.8` there. High contrast now does the same, at
  pure black and pure white. It previously held an inversion dimmed one step off those, which cleared
  the gate but left the theme named for contrast scoring below the default one on its own chip, `Lc
  102.1` against `105.5` in light and `90.3` against `97.5` in dark. At the ink it reaches `Lc 107.9`
  and `106.0`. The separation is still not gated, because the fill it is measured against is a ground
  rather than a foreground, and only the label on the chip is a reading task.

The gate additionally fails if a token exists in one mode but not the other, or if any token falls
outside the sRGB gamut, since a browser would then render a colour other than the one measured here.

## Declared pairs

```
text on bg: 90
text on bg-subtle: 90
text on surface: 90
text on surface-hover: 75
text on accent-soft: 75
text-muted on bg: 75
text-muted on bg-subtle: 75
text-muted on surface: 75
text-muted on surface-hover: 75
text-muted on accent-soft: 75
text on panel: 75
text-muted on panel: 75
text-muted on success-soft: 75
text-muted on danger-soft: 75
text-muted on warning-soft: 75
inverse-contrast on inverse: 90
inverse-muted on inverse: 75
inverse-border on inverse: 15 max 45
selected-contrast on selected: 90
blue-contrast on blue: 75
blue-text on bg: 75
blue-text on surface: 75
blue-text on blue-soft: 75
blue-display on bg: 60 (dark min 45)
text-muted on blue-soft: 75
text on blue-soft: 75

accent-contrast on accent: 75
accent-contrast on accent-hover: 75
accent-text on bg: 75
accent-text on bg-subtle: 75
accent-text on surface: 75
accent-text on surface-hover: 75
accent-text on accent-soft: 75
accent-text on panel: 75

brand-contrast on brand: 45

danger-contrast on danger: 75
danger-contrast on danger-hover: 75
danger-text on bg: 75
danger-text on surface: 75
danger-text on surface-hover: 75
danger-text on danger-soft: 75

success-contrast on success: 75
success-text on bg: 75
success-text on surface: 75
success-text on success-soft: 75

warning-contrast on warning: 75
warning-text on bg: 75
warning-text on surface: 75
warning-text on warning-soft: 75

border on bg: 15 max 45 (dark step 30..75)
border on bg-subtle: 15 max 45 (dark step 30..75)
border on surface: 15 max 45 (dark step 30..75)
border-strong on bg: 45 max 75 (dark min 25)
border-strong on bg-subtle: 45 max 75 (dark min 25)
border-strong on surface: 45 max 75 (dark min 25)
ring on border: 45
ring on bg: 45
ring on bg-subtle: 45
ring on surface: 45
ring on surface-hover: 45
ring on accent-soft: 45
accent on accent-soft: 45
ring on panel: 45
ring on danger-soft: 45
ring on blue-soft: 45
```

## Where each pair comes from

| Pair group | Components |
| --- | --- |
| `text` on the four surface layers | every component's resting text; table rows at rest, on hover and when selected; the switch thumb on the unchecked track. Also used as a border: the secondary button's outline and the segmented control's frame, which are control boundaries rather than rules, so they take the text weight deliberately |
| `text` on `accent-soft` | selected command item |
| `accent` on `accent-soft` | the outline marking the selected command item |
| `text` and `text-muted` on `blue-soft` | the selected row in every list and table, its title and its secondary text |
| `selected-contrast` on `selected` | the label on the chosen segment, in both segmented controls: Installed and Browse on Plugins, and Appearance and Theme in Settings |
| `text-muted` on the surface layers | placeholders, descriptions, menu labels, table headers, shortcuts, scroll buttons |
| `text-muted` on `accent-soft` | shortcut inside a selected command item |
| `text-muted` on the status tints | sonner toast descriptions for success, error and warning |
| `inverse-contrast on inverse` | tooltip; the site's reversed closing panel, which re-scopes `text`, `text-muted`, `border` and the accent pair to the inverse family |
| `inverse-muted` and `inverse-border` on `inverse` | secondary text and the spec rules inside that panel |
| `accent-contrast` on `accent` / `accent-hover` | primary button at rest and on hover, solid accent badge, toast action button |
| `accent-text` on the surface layers and `accent-soft` | link button, check indicators in select and menu items (including when highlighted), soft accent badge, selected command item, text selection; the active tab bar and the focus border on a field |
| `brand-contrast on brand` | the app mark's glyph on its tile; fixed in both modes, gated like the icon it mirrors |
| `danger` group | the destructive button's label on its fill, the outline destructive button's border and label at rest and on its `danger-soft` hover, danger menu item at rest and highlighted, invalid field border, error toast, danger badge |
| `success` / `warning` groups | status badges, toasts, status indicators |
| `border` | row rules, card and popover outlines, the sidebar search trigger, the frame around the site's preview |
| `border-strong` | input and select borders, the unchecked switch track, scrollbar thumb |
| `blue` group | the one hue in the system, and it means "on", "selected" or "needs updating": the checked switch track and checkbox fill and the solid count pill on the collapsed rail beside the Needs updating entry (its label is gated on `blue`), the sidebar entry the app now opens on; the selected row tint (`blue-soft`); the solid stale badge on every list row, table cell and detail pane it can appear in, whose fill is what separates it from a selected row of the same tint, so it carries no hairline; the soft app-update badge on the Settings entry; and the site's rotating hero word (`blue-display`). The fill keeps one value in both modes; `blue-text` (body-size text, unused so far), `blue-display` (display text) and `ring` are the same hue lifted to what each needs on a dark ground, which is why body-size blue text is the palest of the three |
| `ring` beside `border` | the focus outline where it lands next to a row rule or a card outline. It is not declared against `border-strong`, because the ring carries a two-pixel offset and never meets a control's own edge |
| `ring` | the shared focus outline, drawn in the blue family, on the page, on a card, on a highlighted menu item, on a selected command item, and on a highlighted danger menu item |

## Themes

The gate runs the whole list against every theme in both modes, not just the default palette.
Themes are declared in `scripts/check-contrast.mjs`, each naming its light and dark selector in
`src/tokens.css`.

A palette block must declare every token in both of its modes. A partial block would resolve by
source order against `.dark`, which is the one way this file could render a colour the gate never
measured, so the gate rejects an incomplete block. `brand` and `brand-contrast` are the exception in
the other direction: they are the app mark and must render identically everywhere, so no palette may
override them.

Themes sit at one of two tiers.

- **Full.** Every declared pair holds. `default` and `high-contrast` are here. `high-contrast`
  additionally waives the ceilings, because a ceiling exists to stop a rule reaching the weight of
  the text it separates, and that is the rule a high contrast theme is for breaking. It also raises
  every reading pair to `Lc 90`, described below.
- **Cosmetic.** A theme that reproduces a published palette faithfully, where the palette's own
  colours miss some of these levels. `dracula` is here. Its shortfalls are not skipped: every one is
  listed below with the `Lc` it actually reaches, and the gate fails if a listed pair drifts by more
  than 0.5, if a listed pair starts passing, or if a pair fails that is not listed. The list can
  only be wrong loudly.

## The high contrast floor

Clearing the same levels as every other theme is not enough for the theme named after contrast. Held
to the shared list alone, `high-contrast` scored *below* `default` on 16 pairs in light and 4 in
dark, and nothing caught it, because every one of them was passing its declared level. A convention
nobody checks is not a rule.

So `high-contrast` raises every reading pair, meaning every pair declared at `Lc 60` or above, to a
floor of `Lc 90`, the level this system uses for primary text. Pairs declared at `45` and `15` are
not reading tasks and keep their own levels; the ceiling waiver already covers what a high contrast
theme wants from a hairline.

Eleven pairs cannot reach the floor, and they share one shape: getting there would cost the colour
its identity. They are listed below with the `Lc` each reaches, and the gate holds them the same way it
holds a cosmetic deviation. It fails if one drifts by more than 0.5, if one starts clearing `Lc 90`
and should therefore be deleted, or if a pair falls short that is not listed.

- `inverse-muted on inverse` is the dimmed text on the inverse surface. Raising it to `90` is
  raising it to the level of the text beside it, at which point nothing about it is muted.
- `danger-text`, `success-text` and `blue-text` on their own tints and on `surface-hover` are
  already the darkest red, green and blue that stay red, green and blue. Past this they read as
  black.
- `blue-display on bg` is display text at 44px and up, which the `60` level exists for.
- `accent-contrast`, `blue-contrast`, `danger-contrast` and `success-contrast` on their fills are
  labels on solid buttons. The only way to lift them is to darken the fill underneath, which turns
  the blue button black, the danger button brown and the success button near enough the same.

### high-contrast light

inverse-muted on inverse: 76.6
danger-text on surface-hover: 84.6
danger-text on danger-soft: 85.1
success-text on success-soft: 85.0
blue-text on blue-soft: 86.2

### high-contrast dark

blue-display on bg: 76.6
accent-contrast on accent-hover: 77.4
danger-contrast on danger: 78.1
success-contrast on success: 78.8
blue-contrast on blue: 80.1
danger-contrast on danger-hover: 84.0

## Declared deviations

Cosmetic-tier shortfalls, as foreground on background followed by the `Lc` the pair reaches.

`dracula` follows the official specification at draculatheme.com/spec, which publishes both halves
of the theme: Dracula Classic for dark and Alucard Classic for light, plus the five UI background
variants each of them uses. Every colour the spec names is used at its exact published hex, with a
single recorded exception noted under dracula dark, verified by converting to OKLCH and rendering
back. The spec requires exact hex adherence, which is what puts this theme at the cosmetic tier: its
colours were drawn for syntax on a code surface, not to carry UI text at `Lc 75`.

Only tokens the spec does not name are derived, and those are tuned to clear the contract: the two
border weights, the ring, the soft tints, and the inverse pair. Every deviation below is therefore a
pair where the spec's own colours fall short, not one where a derived value was left slack.

### dracula dark

Purple (`#BD93F9`) is the accent, and at mid lightness no label clears `Lc 75` on it in either
polarity. Red (`#FF5555`) has the same problem as the danger fill.

Secondary text is the one place this theme leaves the spec. Comment (`#6272A4`) is what Dracula
names for it, and it was drawn to sit quietly under code: on these grounds it measures between
`Lc 16.6` and `Lc 26.6`, and between `1.94:1` and `3.36:1` as a WCAG ratio, so it misses AA on every
surface in the theme. `text-muted` is therefore Comment's own hue and chroma lifted in lightness
alone, to `oklch(0.78 0.08 270.1)`, which clears `4.5:1` on all ten of its grounds. `Lc 75` stays out
of reach at this hue, so the pairs below are still deviations, at roughly half the distance they
were.

```
text-muted on bg: 59.3
text-muted on bg-subtle: 60.7
text-muted on surface: 56.1
text-muted on surface-hover: 51.9
text-muted on accent-soft: 54.7
text-muted on panel: 56.1
text-muted on success-soft: 55.4
text-muted on danger-soft: 56.3
text-muted on warning-soft: 56.1
blue-text on blue-soft: 72.5
text-muted on blue-soft: 50.8
accent-contrast on accent: 51.5
selected-contrast on selected: 52.5
accent-contrast on accent-hover: 52.5
accent-text on bg: 50.7
accent-text on bg-subtle: 52.1
accent-text on surface: 47.5
accent-text on surface-hover: 43.3
accent-text on accent-soft: 46.1
accent-text on panel: 47.5
danger-contrast on danger: 41.6
danger-contrast on danger-hover: 32.9
danger-text on bg: 40.4
danger-text on surface: 37.2
danger-text on surface-hover: 33.0
danger-text on danger-soft: 37.4
warning-contrast on warning: 68.5
```

### dracula light

Alucard's text colours were drawn for syntax on a code surface and none of them carried UI text at
`Lc 75`: every text token in the theme fell short, twenty two pairs in all, with Comment
(`#6C664B`) missing on every tinted ground and on the sidebar. Lifting the grounds instead was not
available, because Selection would have had to reach `L 0.982` against a page ground of `0.987` and
`surface-hover` could not reach `Lc 75` at any lightness. So the five text tokens are Alucard's own
hues and chromas darkened in lightness alone, and Selection is lifted from `#CFCFDE` to `#E2E2F1`.

That takes the shortfalls from twenty two to the six below, and no reading pair in the theme misses
WCAG AA any longer. `selected-contrast on selected` cannot be moved at all: both halves are spec
colours and the level is `90`.

```
text-muted on surface-hover: 73.2
text-muted on blue-soft: 69.8
blue-text on blue-soft: 63.0
accent-text on surface-hover: 73.2
danger-text on surface-hover: 70.5
selected-contrast on selected: 86.7
```

