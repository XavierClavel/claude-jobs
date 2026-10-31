import { useEffect, useState } from 'react'
import { Icon } from './Icon'

// Checks the releases now instead of at the next 6-hourly check. A newer version shows in the update bar; `onCheck`
// resolves true when the app is up to date, which is said here for a moment.
export function UpdateCheck({ onCheck }: { onCheck: () => Promise<boolean> }) {
  const [checking, setChecking] = useState(false)
  const [upToDate, setUpToDate] = useState(false)

  useEffect(() => {
    if (!upToDate) return
    const timer = setTimeout(() => setUpToDate(false), 4000)
    return () => clearTimeout(timer)
  }, [upToDate])

  async function check() {
    setChecking(true)
    setUpToDate(false)
    const current = await onCheck()
    setChecking(false)
    setUpToDate(current)
  }

  return (
    <>
      <button
        type="button"
        className={checking ? 'icon spinning' : 'icon'}
        aria-label="Check for updates"
        title="Check for updates"
        disabled={checking}
        onClick={check}
      >
        <Icon name="refresh" />
      </button>
      {upToDate && (
        <span className="muted" role="status">
          Up to date
        </span>
      )}
    </>
  )
}
