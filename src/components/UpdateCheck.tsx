import { useEffect, useState } from 'react'

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
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <path
            d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M12.5 1.5v3h-3"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {upToDate && (
        <span className="muted" role="status">
          Up to date
        </span>
      )}
    </>
  )
}
