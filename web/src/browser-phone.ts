import {
	Invitation,
	Inviter,
	Registerer,
	RegistererState,
	type Session,
	SessionState,
	UserAgent,
	Web,
} from "sip.js";
import type { SipCredentials } from "./contracts";

const SIP_BUSY_HERE = 486;
const TAKEOVER_WINDOW_MS = 10000;

export interface BrowserState {
	readonly phase:
		| "offline"
		| "registering"
		| "ready"
		| "incoming"
		| "dialing"
		| "talking"
		| "error";
	readonly muted: boolean;
	readonly audioBlocked: boolean;
}
export class BrowserPhone {
	private agent: UserAgent | undefined;
	private registerer: Registerer | undefined;
	private session: Session | undefined;
	private audio: HTMLAudioElement | undefined;
	private acceptUntil = 0;
	private state: BrowserState = {
		phase: "offline",
		muted: false,
		audioBlocked: false,
	};
	private listeners = new Set<() => void>();
	subscribe = (listener: () => void): (() => void) => {
		this.listeners.add(listener);
		return (): void => {
			this.listeners.delete(listener);
		};
	};
	getSnapshot = (): BrowserState => this.state;
	private publish(update: Partial<BrowserState>): void {
		this.state = { ...this.state, ...update };
		for (const listener of this.listeners) listener();
	}
	setAudio = (audio: HTMLAudioElement | null): void => {
		this.audio = audio ?? undefined;
	};
	async enable(credentials: SipCredentials): Promise<void> {
		if (this.agent) return;
		this.publish({ phase: "registering" });
		const uri = UserAgent.makeURI(
			`sip:${credentials.username}@${credentials.domain}`,
		);
		if (!uri) throw new Error("電話の接続情報が無効です。");
		const agent = new UserAgent({
			uri,
			authorizationUsername: credentials.username,
			authorizationPassword: credentials.password,
			logBuiltinEnabled: false,
			transportOptions: {
				server: `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/sip`,
			},
			sessionDescriptionHandlerFactoryOptions: {
				constraints: { audio: true, video: false },
				peerConnectionConfiguration: { iceServers: [] },
			},
			delegate: {
				onInvite: (invitation: Invitation): void => {
					if (this.session) {
						void invitation.reject({ statusCode: SIP_BUSY_HERE });
						return;
					}
					this.bind(invitation);
					this.publish({ phase: "incoming" });
					if (Date.now() <= this.acceptUntil) {
						this.acceptUntil = 0;
						void this.answer().catch(() => this.publish({ phase: "error" }));
					}
				},
				onDisconnect: (): void => this.publish({ phase: "error" }),
			},
		});
		this.agent = agent;
		const registerer = new Registerer(agent);
		this.registerer = registerer;
		registerer.stateChange.addListener((state: RegistererState): void => {
			if (state === RegistererState.Registered)
				this.publish({ phase: "ready" });
		});
		try {
			await agent.start();
			await registerer.register();
		} catch (error) {
			await this.disable();
			this.publish({ phase: "error" });
			throw error;
		}
	}
	private bind(session: Session): void {
		this.session = session;
		session.stateChange.addListener((state: SessionState): void => {
			if (this.session !== session) return;
			if (state === SessionState.Established) {
				this.publish({ phase: "talking", muted: false });
				const handler = session.sessionDescriptionHandler;
				if (handler instanceof Web.SessionDescriptionHandler && this.audio) {
					this.audio.srcObject = handler.remoteMediaStream;
					void this.audio
						.play()
						.catch(() => this.publish({ audioBlocked: true }));
				}
			}
			if (state === SessionState.Terminated) {
				this.session = undefined;
				if (this.audio) {
					this.audio.srcObject = null;
				}
				this.publish({
					phase: this.agent ? "ready" : "offline",
					muted: false,
					audioBlocked: false,
				});
			}
		});
	}
	async answer(): Promise<void> {
		const session = this.session;
		if (!(session instanceof Invitation))
			throw new Error("応答できる着信がありません。");
		this.publish({ phase: "dialing" });
		await session.accept();
	}
	async prepareTakeover(): Promise<void> {
		if (!this.agent) throw new Error("ブラウザー通話を有効にしてください。");
		const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
		for (const track of stream.getTracks()) track.stop();
		this.acceptUntil = Date.now() + TAKEOVER_WINDOW_MS;
	}
	async dial(number: string): Promise<void> {
		if (!this.agent || this.session)
			throw new Error("ブラウザー通話の接続を確認してください。");
		if (!/^\+?[0-9]{1,32}$/.test(number))
			throw new Error("電話番号を数字で入力してください。");
		const domain = this.agent.configuration.uri.host;
		const uri = UserAgent.makeURI(`sip:${number}@${domain}`);
		if (!uri) throw new Error("電話番号が無効です。");
		const session = new Inviter(this.agent, uri);
		this.bind(session);
		this.publish({ phase: "dialing" });
		try {
			await session.invite();
		} catch (error) {
			this.session = undefined;
			this.publish({ phase: "ready" });
			throw error;
		}
	}
	mute = (): void => {
		const handler = this.session?.sessionDescriptionHandler;
		if (!(handler instanceof Web.SessionDescriptionHandler)) return;
		const muted = !this.state.muted;
		for (const sender of handler.peerConnection?.getSenders() ?? []) {
			if (sender.track?.kind === "audio") sender.track.enabled = !muted;
		}
		this.publish({ muted });
	};
	async playAudio(): Promise<void> {
		await this.audio?.play();
		this.publish({ audioBlocked: false });
	}
	async hangup(): Promise<void> {
		const session = this.session;
		if (!session) return;
		if (session.state === SessionState.Established) await session.bye();
		else if (session instanceof Invitation) await session.reject();
		else if (session instanceof Inviter) await session.cancel();
	}
	async disable(): Promise<void> {
		this.acceptUntil = 0;
		try {
			await this.hangup();
			await this.registerer?.unregister();
		} finally {
			await this.agent?.stop();
			this.agent = undefined;
			this.registerer = undefined;
			this.session = undefined;
			if (this.audio) this.audio.srcObject = null;
			this.publish({ phase: "offline", muted: false, audioBlocked: false });
		}
	}
}
export const browserPhone = new BrowserPhone();
