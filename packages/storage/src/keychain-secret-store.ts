export interface SecretStore {
  get(): Promise<string | null>;
  set(value: string): Promise<void>;
  delete(): Promise<void>;
}

type KeytarModule = {
  getPassword(service: string, account: string): Promise<string | null>;
  setPassword(service: string, account: string, password: string): Promise<void>;
  deletePassword(service: string, account: string): Promise<boolean>;
};

export class KeytarSecretStore implements SecretStore {
  constructor(
    private readonly service: string,
    private readonly account: string
  ) {}

  async get(): Promise<string | null> {
    const keytar = await loadKeytar();
    return keytar.getPassword(this.service, this.account);
  }

  async set(value: string): Promise<void> {
    const keytar = await loadKeytar();
    await keytar.setPassword(this.service, this.account, value);
  }

  async delete(): Promise<void> {
    const keytar = await loadKeytar();
    await keytar.deletePassword(this.service, this.account);
  }
}

async function loadKeytar(): Promise<KeytarModule> {
  try {
    const module = (await import("keytar")) as unknown as KeytarModule | { default: KeytarModule };
    return "default" in module ? module.default : module;
  } catch (error) {
    throw new Error(
      `系统钥匙串支持不可用。请确认 optional dependency "keytar" 已安装。${error instanceof Error ? ` ${error.message}` : ""}`
    );
  }
}
