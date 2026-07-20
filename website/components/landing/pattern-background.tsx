export function PatternBackground() {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden opacity-60">
      <div className="absolute left-[-18%] top-10 h-72 w-72 rounded-full bg-[rgba(155,120,82,0.15)] blur-3xl" />
      <div className="absolute right-[-12%] top-48 h-80 w-80 rounded-full bg-[rgba(47,109,88,0.14)] blur-3xl" />
      <div
        className="absolute inset-0"
        style={{
          backgroundImage: "url('/pattern.svg')",
          backgroundSize: '240px',
          backgroundPosition: 'center',
          maskImage: 'linear-gradient(to bottom, rgba(0,0,0,0.52), transparent 78%)'
        }}
      />
    </div>
  )
}
