import type { ReactNode } from 'react'

// The parts a docs page is written with (app/docs/content.tsx).
export const Steps = ({ children }: { children: ReactNode }) => <ol className="doc-steps">{children}</ol>
export const Note = ({ children, tone = 'info' }: { children: ReactNode; tone?: 'info' | 'soon' }) => (
  <div className={`doc-note doc-note-${tone}`}>{children}</div>
)
export const Cmd = ({ children }: { children: string }) => <code className="doc-cmd">{children}</code>
