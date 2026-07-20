import { LegalPage } from '@/components/landing/legal-page'

export default function LicenseInformationPage() {
  return (
    <LegalPage
      title="License Information"
      intro="A placeholder licensing page for the open-source and third-party software used by the Qurb Cloud ecosystem."
      sections={[
        {
          heading: 'Open source acknowledgement',
          body: 'This page can later list repository licenses, disclosures, and attribution statements for software packages used in the desktop and mobile stack.'
        },
        {
          heading: 'Project licensing',
          body: 'As the company matures, it can explain the product licensing model, what is open, and what is proprietary in a simple and transparent way.'
        }
      ]}
    />
  )
}
