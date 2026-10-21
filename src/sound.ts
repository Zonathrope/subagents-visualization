let ctx: AudioContext | null = null

/**
 * The "needs you" chime: two short rising tones, generated so no audio files ship.
 * Silently does nothing if audio is unavailable.
 */
export function playUrgent() {
  try {
    ctx ??= new AudioContext()
    const start = ctx.currentTime
    for (const [i, freq] of [660, 880].entries()) {
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.type = 'sine'
      osc.frequency.value = freq
      const t = start + i * 0.16
      gain.gain.setValueAtTime(0.0001, t)
      gain.gain.exponentialRampToValueAtTime(0.18, t + 0.02)
      gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.14)
      osc.connect(gain).connect(ctx.destination)
      osc.start(t)
      osc.stop(t + 0.15)
    }
  } catch {
    /* no audio device or blocked */
  }
}
