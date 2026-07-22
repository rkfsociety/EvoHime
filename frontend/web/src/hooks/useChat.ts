import { useRef, useState } from "react";
import type { SessionBootstrap } from "../protocol";
import type { ChatLine, ChatSessionSummary } from "../types";

const initialLines: ChatLine[] = [];

export function useChat() {
  const [session, setSession] = useState<SessionBootstrap | null>(null);
  const [chatSessions, setChatSessions] = useState<ChatSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [lines, setLines] = useState<ChatLine[]>(initialLines);
  const [stream, setStream] = useState("");
  const [chatActionNotice, setChatActionNotice] = useState<string | null>(null);
  const [composerNotice, setComposerNotice] = useState<string | null>(null);
  const [attachments, setAttachments] = useState<File[]>([]);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [archivedChats, setArchivedChats] = useState<ChatSessionSummary[]>([]);
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const sessionLoadRef = useRef(0);
  const chatLogRef = useRef<HTMLDivElement | null>(null);
  const chatAutoScrollRef = useRef(true);

  return {
    session,
    setSession,
    chatSessions,
    setChatSessions,
    activeSessionId,
    setActiveSessionId,
    input,
    setInput,
    lines,
    setLines,
    stream,
    setStream,
    chatActionNotice,
    setChatActionNotice,
    composerNotice,
    setComposerNotice,
    attachments,
    setAttachments,
    deletingSessionId,
    setDeletingSessionId,
    archivedChats,
    setArchivedChats,
    attachmentInputRef,
    sessionLoadRef,
    chatLogRef,
    chatAutoScrollRef,
  };
}
