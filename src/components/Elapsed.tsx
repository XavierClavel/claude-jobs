import { useEffect, useState } from 'react'
import { stopwatch } from '../format'

// How long a run has been going, ticking every second.
export function Elapsed({ since }: { since: string }) {
  const start = new Date(since).getTime()
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>
    const tick = () => {
      const at = Date.now()
      setNow(at)
      // On the second the run started at, so the count turns over when it should.
      timer = setTimeout(tick, 1000 - ((((at - start) % 1000) + 1000) % 1000))
    }
    tick()
    return () => clearTimeout(timer)
  }, [start])
  return <span className="elapsed">{stopwatch((now - start) / 1000)}</span>
}
