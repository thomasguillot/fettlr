import { beforeEach, describe, expect, it, vi } from "vitest";

import type { UiMcpServer } from "../../bindings";
import type { CommandResult } from "../../store";

const revealSecret = vi.fn();
const editMcpServer = vi.fn();

vi.mock("../../bindings", () => ({
  commands: {
    revealSecret: (...args: unknown[]) => revealSecret(...args),
    editMcpServer: (...args: unknown[]) => editMcpServer(...args),
  },
}));

const { repinMcpServer } = await import("./mcpConfig");

const server: UiMcpServer = {
  id: "user:fetch",
  name: "fetch",
  origin: { kind: "userScope" },
  transport: "stdio",
  isEnabled: true,
  config: {
    kind: "stdio",
    command: "npx",
    args: ["-y", "@scope/fetch@1.0.0"],
    maskedArgs: [],
    url: null,
    urlMasked: false,
    env: {},
    envKeys: ["TOKEN"],
    headers: {},
    headerKeys: [],
  },
};

const ok = <T>(data: T): CommandResult<T> => ({ status: "ok", data });
const dispatch = async <T>(command: Promise<CommandResult<T>>): Promise<T | undefined> => {
  const result = await command;
  return result.status === "ok" ? result.data : undefined;
};

beforeEach(() => {
  revealSecret.mockReset();
  editMcpServer.mockReset();
});

describe("repinMcpServer", () => {
  it("rewrites the pin and carries the revealed secrets across", async () => {
    revealSecret.mockResolvedValue(ok("s3cret"));
    editMcpServer.mockResolvedValue(ok({ snapshot: true }));

    const result = await repinMcpServer(dispatch, server, "2.0.0");

    expect(result).toEqual({ snapshot: true });
    expect(revealSecret).toHaveBeenCalledWith("user:fetch", "env", "TOKEN", null);
    expect(editMcpServer).toHaveBeenCalledWith("user:fetch", {
      name: "fetch",
      origin: { kind: "userScope" },
      config: {
        kind: "stdio",
        command: "npx",
        args: ["-y", "@scope/fetch@2.0.0"],
        url: null,
        env: { TOKEN: "s3cret" },
        headers: null,
      },
    });
  });

  it("reveals masked arguments and the URL before it rewrites and saves", async () => {
    const masked: UiMcpServer = {
      ...server,
      config: {
        ...server.config,
        args: ["-y", "@scope/fetch@1.0.0", "--token", "•••"],
        maskedArgs: [3],
        url: "https://host/mcp?k=•••",
        urlMasked: true,
        envKeys: [],
      },
    };
    revealSecret.mockImplementation(async (_id: string, field: string) =>
      ok(field === "url" ? "https://host/mcp?k=real" : "sk-real"),
    );
    editMcpServer.mockResolvedValue(ok({ snapshot: true }));

    await repinMcpServer(dispatch, masked, "2.0.0");

    expect(revealSecret).toHaveBeenCalledWith("user:fetch", "args", "3", "•••");
    expect(revealSecret).toHaveBeenCalledWith("user:fetch", "url", "", "https://host/mcp?k=•••");
    expect(editMcpServer).toHaveBeenCalledWith(
      "user:fetch",
      expect.objectContaining({
        config: expect.objectContaining({
          args: ["-y", "@scope/fetch@2.0.0", "--token", "sk-real"],
          url: "https://host/mcp?k=real",
        }),
      }),
    );
  });

  it("does nothing when the arguments carry no pin", async () => {
    const unpinned = { ...server, config: { ...server.config, args: ["-y", "@scope/fetch"] } };

    const result = await repinMcpServer(dispatch, unpinned, "2.0.0");

    expect(result).toBeUndefined();
    expect(revealSecret).not.toHaveBeenCalled();
    expect(editMcpServer).not.toHaveBeenCalled();
  });

  it("does not save when a secret cannot be revealed", async () => {
    revealSecret.mockResolvedValue({ status: "error", error: { message: "gone", detail: null } });

    const result = await repinMcpServer(dispatch, server, "2.0.0");

    expect(result).toBeUndefined();
    expect(editMcpServer).not.toHaveBeenCalled();
  });
});
