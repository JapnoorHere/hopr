/** Command palette open/close state. */
class PaletteState {
  open = $state(false);
  /** Text the palette opens with (e.g. ">" for actions only). */
  initial = $state("");

  show(initial = "") {
    this.initial = initial;
    this.open = true;
  }

  close() {
    this.open = false;
  }
}

export const palette = new PaletteState();
