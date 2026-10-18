import { ChatView } from '@/components/chat/chat-view';

export default async function ConversationPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <ChatView key={id} conversationId={id} />;
}
