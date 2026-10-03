import { icons, type IconName } from './icon-paths'

/**
 * A Lucide icon, drawn as the window draws them (direction §10): a 1.75
 * stroke, round caps and joins, no fill, in the current colour.
 */
export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      className={className ? `i ${className}` : 'i'}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: icons[name] }}
    />
  )
}
