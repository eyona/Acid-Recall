# ACID RECALL — Sampler and extended patterns

[Studio guide](studio.md) · [MIDI controller setup](midi-controller.md)

## TR-808 and TR-909

All voice knobs, sound switches and mixer strips retain their existing positions.
The sound switches use **CON**, **CLA** and **MAR**; hover for their full names.

**LENGTH 16** is the default. Choose **32** or **64** to reveal two or four pages of
sixteen steps. Click a page miniature to edit it; **FOLLOW** follows the playback page.
Clicking a page manually turns FOLLOW off so playback does not move your editing view.

**COPY PAGE / PASTE PAGE** copy all twelve instrument lanes, including accents and 909
velocity/flam values. **DUPLICATE** copies to the next page within the chosen length.
Choose a longer length first if you want to extend the final page.

Reducing the length hides later pages without deleting their notes. Increasing it again
restores them. Groove Generator and RND generate the entire selected 16/32/64-step phrase;
**Undo Random** restores the previous phrase while there have been no intervening edits.

Save extended patterns in **Studio .recall** projects. New Save As operations use `.recall`
by default. Existing `.orb338` projects still open, and Save keeps an opened legacy file's
name and extension. The binary container format is unchanged. The legacy .rbs format
contains only the first sixteen steps.

## TB-303

The note-key layout is unchanged. **A** marks Accent, **S** marks Slide. Right-click a key
to choose a note from the grid (**C, C#, D… B, C+**), with Rest, Accent, Slide and octave flags.
C+ preserves the original upper-C pitch value.

## Sampler Focus

Choose a lane on the left. Its name, waveform and LOAD/AUDITION controls are grouped together.
ONE SHOT, AUTO CUT, GATE, LOOP and REVERSE remain available, with Pitch/Level/Reverb knobs.
WARP has its own timing row; MARKERS / GRID opens the detailed editor. The sequence sits
below the editing area. The MIX strip remains on the right.

Expand **GATER / SIDECHAIN** for modulation settings. GATER and SIDECHAIN have independent
source choices. For SIDECHAIN, choose **GLOBAL** to retain the shared sidechain configuration,
or choose **TR-808 / TR-909** and an instrument such as BD or SD for this sample lane only.
The explicit source works even when the global TB sidechain is disabled.

The effects column remains on the right. SPACE uses two columns to keep labels separate.
The master panel is **MASTER**, with a **COMP** control for its compressor. Compact routing
button labels fit inside their original click areas.
