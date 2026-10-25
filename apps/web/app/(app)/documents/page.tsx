'use client';

import { DocumentLibrary, DocumentUpload } from '@/components/documents';
import { Loaded, PageHeader } from '@/components/ui';
import { useApiData, type DocumentList } from '@/lib/api';

export default function DocumentsPage() {
  const state = useApiData<DocumentList>('/documents');

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Documents"
        description="Les procédures, notes et chartes de l’entreprise que l’agent consulte pour vous répondre, dans vos droits : il cite ceux dont il se sert."
      />
      <Loaded state={state}>
        {({ documents, truncated, spaces, quota, maxFileBytes }) => (
          <>
            {spaces.length > 0 && (
              <DocumentUpload
                spaces={spaces}
                quota={quota}
                maxFileBytes={maxFileBytes}
                onDeposited={state.reload}
              />
            )}
            <DocumentLibrary documents={documents} truncated={truncated} onChanged={state.reload} />
          </>
        )}
      </Loaded>
    </div>
  );
}
