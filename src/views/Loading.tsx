import { useEffect, useState } from 'react'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

type Stage = 'checking' | 'downloading' | 'done' | 'error'

export default function Loading({ onDone }: { onDone: () => void }) {
  const [progress, setProgress] = useState(0)
  const [fadeOut, setFadeOut] = useState(false)
  const [_stage, setStage] = useState<Stage>('checking')
  const [statusText, setStatusText] = useState('Checking for updates...')

  const finish = () => {
    setFadeOut(true)
    setTimeout(onDone, 500)
  }

  useEffect(() => {
    const run = async () => {
      try {
        const update = await check()

        if (update?.available) {
          setStage('downloading')
          setStatusText(`Downloading v${update.version}...`)

          let downloaded = 0
          let total = 0

          await update.downloadAndInstall((event) => {
            if (event.event === 'Started') {
              total = event.data.contentLength ?? 0
            } else if (event.event === 'Progress') {
              downloaded += event.data.chunkLength
              if (total > 0) {
                setProgress(Math.round((downloaded / total) * 100))
                setStatusText(`Downloading v${update.version}... ${Math.round((downloaded / total) * 100)}%`)
              }
            } else if (event.event === 'Finished') {
              setStatusText('Installing update...')
              setProgress(100)
            }
          })

          setStatusText('Restarting...')
          await relaunch()
        } else {
          setStage('done')
          setStatusText('')

          const duration = 2500
          const interval = 30
          const steps = duration / interval
          let current = 0

          const timer = setInterval(() => {
            current++
            setProgress(Math.min(Math.round((current / steps) * 100), 100))
            if (current >= steps) {
              clearInterval(timer)
              finish()
            }
          }, interval)

          return () => clearInterval(timer)
        }
      } catch (err) {
        console.error('Error checking updates:', err)
        setStage('done')
        setStatusText('')

        const duration = 2500
        const interval = 30
        const steps = duration / interval
        let current = 0

        const timer = setInterval(() => {
          current++
          setProgress(Math.min(Math.round((current / steps) * 100), 100))
          if (current >= steps) {
            clearInterval(timer)
            finish()
          }
        }, interval)

        return () => clearInterval(timer)
      }
    }

    run()
  }, [])

return (
  <div
    className="fixed inset-0 z-50 flex flex-col items-center justify-center bg-[#081e0f] transition-opacity duration-500"
    style={{ opacity: fadeOut ? 0 : 1, pointerEvents: fadeOut ? 'none' : 'all' }}
  >
    <div
      className="flex flex-col items-center gap-6 transition-all duration-700"
      style={{
        transform: fadeOut ? 'scale(.05)' : 'scale(1)',
        opacity: fadeOut ? 0 : 1,
      }}
    >
      <div className="flex items-center gap-3">
        <img
          src="./icon.png"
          className="w-9 h-9 object-contain"
          alt=""
        />
        <img
          src="./modstack-title.png"
          className="h-9 object-contain"
          alt="Modstack"
        />
      </div>

      <div className="flex flex-col items-center gap-3 w-64">
        <div className="w-full h-[5px] bg-white/5 rounded-full overflow-hidden">
          <div
            className="h-full bg-[#1bd96a] rounded-full transition-all duration-100"
            style={{ width: `${progress}%` }}
          />
        </div>
        <p className="text-[11px] text-white/25 font-medium tracking-widest uppercase">
          {statusText}
        </p>
      </div>
    </div>
  </div>
 )
}