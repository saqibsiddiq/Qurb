const rows = [
  {
    label: 'Subscription',
    google: '$1.99/month+',
    dropbox: '$9.99/month+',
    icloud: '$0.99/month+',
    onedrive: '$1.99/month+',
    qurb: 'Own your hardware'
  },
  {
    label: 'Privacy',
    google: 'Ad-supported product design',
    dropbox: 'Provider-controlled',
    icloud: 'Closed ecosystem',
    onedrive: 'Platform-owned',
    qurb: 'User-owned, private-first'
  },
  {
    label: 'Storage ownership',
    google: 'Remote vendor',
    dropbox: 'Remote vendor',
    icloud: 'Remote vendor',
    onedrive: 'Remote vendor',
    qurb: 'Your desktop becomes the cloud'
  },
  {
    label: 'Offline',
    google: 'Partial',
    dropbox: 'Partial',
    icloud: 'Partial',
    onedrive: 'Partial',
    qurb: 'Native local-first'
  },
  {
    label: 'Open architecture',
    google: 'Locked',
    dropbox: 'Locked',
    icloud: 'Closed',
    onedrive: 'Closed',
    qurb: 'Open, flexible, self-hosting ready'
  },
  {
    label: 'Self-hosted',
    google: 'No',
    dropbox: 'No',
    icloud: 'No',
    onedrive: 'No',
    qurb: 'Yes'
  }
]

export function ComparisonTable() {
  return (
    <div className="overflow-x-auto rounded-[28px] border border-[var(--border)] bg-[var(--panel)]">
      <table className="min-w-full text-left text-sm">
        <thead>
          <tr className="border-b border-[var(--border)] text-[var(--muted)]">
            <th className="p-4">Category</th>
            <th className="p-4">Google Drive</th>
            <th className="p-4">Dropbox</th>
            <th className="p-4">iCloud</th>
            <th className="p-4">OneDrive</th>
            <th className="p-4">Qurb Cloud</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.label} className="border-b border-[var(--border)] last:border-b-0">
              <td className="p-4 font-medium text-[var(--text)]">{row.label}</td>
              <td className="p-4 text-[var(--muted)]">{row.google}</td>
              <td className="p-4 text-[var(--muted)]">{row.dropbox}</td>
              <td className="p-4 text-[var(--muted)]">{row.icloud}</td>
              <td className="p-4 text-[var(--muted)]">{row.onedrive}</td>
              <td className="p-4 font-medium text-[var(--accent)]">{row.qurb}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
