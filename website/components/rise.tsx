import { Fragment, type CSSProperties } from 'react'

// A headline whose words rise into place, one after another, when it comes
// into view (depth.tsx marks it). Each line its own; `accent` is the line in
// green. Without the script, or with reduced motion, it is simply there.
export function Rise({ as: Tag = 'h2', lines, accent }: { as?: 'h1' | 'h2'; lines: string[]; accent?: number }) {
  let i = 0
  return (
    <Tag className="rise">
      {lines.map((line, li) => (
        <span key={li} className={`rise-line${li === accent ? ' rise-accent' : ''}`}>
          {line.split(' ').map((word, wi, all) => (
            <Fragment key={wi}>
              <span className="rise-w" style={{ '--i': i++ } as CSSProperties}>
                <span>{word}</span>
              </span>
              {wi < all.length - 1 && ' '}
            </Fragment>
          ))}
        </span>
      ))}
    </Tag>
  )
}
