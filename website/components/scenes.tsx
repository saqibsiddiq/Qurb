/**
 * Two scenes for the landing page, drawn from the apps' parts like the
 * previews beside them: a send crossing from a phone to a computer and
 * leaving nothing behind, and a guest's folder on a shared computer, opened
 * from the guest's phone. The words are the apps' own (the window's
 * devices.js, the phone's MainActivity and ShareActivity); the people, files
 * and devices are made up. Each is one image to a screen reader.
 */
import { Icon } from './icon'
import { Rows } from './previews'

/** A file sent to one device: read where it is, carried across encrypted,
 *  delivered there and nowhere else, with no copy kept (decision 0060). */
export function SendScene() {
  return (
    <div
      className="send-scene glass-frosted"
      role="img"
      aria-label="Tickets.pdf goes from a phone to a laptop, encrypted, straight across. It arrives in the laptop's Downloads; it went to that laptop only, and Qurb kept no copy."
    >
      <div className="end">
        <span className="tile big">
          <Icon name="smartphone" size={26} />
        </span>
        <strong>Phone</strong>
        <span className="chip">
          <Icon name="file-text" size={15} />
          Tickets.pdf
        </span>
        <span className="note">Read where it is</span>
      </div>

      <div className="route" aria-hidden="true">
        <span className="track" />
        <span className="packet">
          <Icon name="file-text" size={15} />
        </span>
        <span className="caption">Encrypted, straight across</span>
      </div>

      <div className="end">
        <span className="tile big">
          <Icon name="laptop" size={26} />
        </span>
        <strong>Laptop</strong>
        <span className="chip arrived">
          <Icon name="circle-check" size={15} />
          In Downloads
        </span>
        <span className="note">The only one to get it</span>
      </div>

      <div className="ledger">
        <span>
          <Icon name="check" size={16} />
          Delivered to Laptop, and nowhere else
        </span>
        <span>
          <Icon name="check" size={16} />
          No copy kept by Qurb
        </span>
      </div>
    </div>
  )
}

/** A guest's folder on a shared computer: sealed, so the computer reads
 *  nothing; opened at the computer only when the guest's phone approves,
 *  behind its screen lock (decision 0060, step 5). */
export function GuestScene() {
  return (
    <div
      className="guest-scene"
      role="img"
      aria-label="On the phone: Open your folder on Saqib's laptop? with Open it there, confirmed with a fingerprint. On the laptop, before: the folder kept sealed, names unreadable. After: the folder open, with its files, and a Lock button."
    >
      <div className="phone ask">
        <div className="screen">
          <div className="bar">
            <span>12:30</span>
            <span>5G</span>
          </div>
          <div className="sheet glass-elevated">
            <span className="grab" />
            <span className="tile big">
              <Icon name="lock-keyhole" size={24} />
            </span>
            <h3>Open your folder on Saqib’s laptop?</h3>
            <p>
              Your folder is kept on Saqib’s laptop sealed: it can’t open it. Only approve if you’re there and asked
              for it.
            </p>
            <span className="btn primary inert">Open it there</span>
            <span className="btn ghost inert">Not now</span>
            <span className="finger">
              <span className="ring">
                <Icon name="fingerprint" size={26} />
              </span>
              Confirm it’s you
            </span>
          </div>
        </div>
      </div>

      <div className="folders-pair">
        <div className="panel glass-frosted sealed">
          <div className="panel-head">
            <strong>What the laptop holds</strong>
            <span className="state">
              <Icon name="lock-keyhole" size={14} />
              Sealed
            </span>
          </div>
          <ul className="sealed-names">
            {['~d3Kq9vXl0bTa7wR2', '~Pz81mYcE4sQnJ5uH', '~a7FvL0xW2kRt9eNc'].map((name) => (
              <li key={name}>
                <span className="tile">
                  <Icon name="eye-off" size={16} />
                </span>
                <span className="mono">{name}</span>
              </li>
            ))}
          </ul>
        </div>

        <div className="panel glass-elevated open">
          <div className="panel-head">
            <strong>Folder kept for Ammi’s phone</strong>
            <span className="presence on">
              <span className="dot" />
              Open
            </span>
          </div>
          <p className="quiet">
            Open here. Anyone at this computer can see these files until you lock it. It locks itself after ten minutes
            unused.
          </p>
          <Rows
            rows={[
              { icon: 'file-text', name: 'recipes/nihari.txt', sub: ['4.1 KB', 'Yesterday'] },
              { icon: 'file-image', name: 'photos/eid.jpg', sub: ['2.8 MB', '4 days ago'] },
              { icon: 'file-text', name: 'letters/for-saqib.txt', sub: ['12 KB', 'Last week'] }
            ]}
          />
          <div className="actions">
            <span className="btn primary inert">
              <Icon name="lock-keyhole" size={16} />
              Lock
            </span>
          </div>
        </div>
      </div>
    </div>
  )
}
