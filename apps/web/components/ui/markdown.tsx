import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/** Text written by the agent, in Markdown; any HTML in it is left out. */
export function Markdown({ children }: { children: string }) {
  return (
    <div className="prose-answer text-[0.95rem]">
      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
        {children}
      </ReactMarkdown>
    </div>
  );
}
