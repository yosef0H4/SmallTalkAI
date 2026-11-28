export const MIN_TYPING_WPM = 50;
export const MIDDLE_TYPING_WPM = 2000;
export const MAX_TYPING_WPM = 4000;

/** Slider position 100 is Instant; finite speeds occupy positions 0 through 99. */
export function positionToWpm(position: number): number {
  const value = Math.max(0, Math.min(99, Math.round(position)));
  return value <= 50
    ? MIN_TYPING_WPM + Math.round(value * (MIDDLE_TYPING_WPM - MIN_TYPING_WPM) / 50)
    : MIDDLE_TYPING_WPM + Math.round((value - 50) * (MAX_TYPING_WPM - MIDDLE_TYPING_WPM) / 49);
}

export function wpmToPosition(wpm: number): number {
  const value = Math.max(MIN_TYPING_WPM, Math.min(MAX_TYPING_WPM, wpm));
  return value <= MIDDLE_TYPING_WPM
    ? Math.round((value - MIN_TYPING_WPM) * 50 / (MIDDLE_TYPING_WPM - MIN_TYPING_WPM))
    : 50 + Math.round((value - MIDDLE_TYPING_WPM) * 49 / (MAX_TYPING_WPM - MIDDLE_TYPING_WPM));
}
