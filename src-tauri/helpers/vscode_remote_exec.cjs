'use strict';

// SPDX-FileCopyrightText: 2026 Sheldon Van
// SPDX-License-Identifier: Apache-2.0

const net = require('net');

const NEED_MORE = Symbol('need_more');

function ensure(buffer, offset, length) {
  if (offset + length > buffer.length) {
    throw NEED_MORE;
  }
}

function readUInt64(buffer, offset) {
  ensure(buffer, offset, 8);
  const value = (BigInt(buffer.readUInt32BE(offset)) << 32n) | BigInt(buffer.readUInt32BE(offset + 4));
  return value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value.toString();
}

function readInt64(buffer, offset) {
  ensure(buffer, offset, 8);
  const value = buffer.readBigInt64BE(offset);
  return value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER)
    ? Number(value)
    : value.toString();
}

function decodeOne(buffer, offset = 0) {
  ensure(buffer, offset, 1);
  const marker = buffer[offset++];

  if (marker <= 0x7f) {
    return [marker, offset];
  }
  if (marker >= 0xe0) {
    return [marker - 0x100, offset];
  }
  if (marker >= 0xa0 && marker <= 0xbf) {
    return readString(buffer, offset, marker & 0x1f);
  }
  if (marker >= 0x90 && marker <= 0x9f) {
    return readArray(buffer, offset, marker & 0x0f);
  }
  if (marker >= 0x80 && marker <= 0x8f) {
    return readMap(buffer, offset, marker & 0x0f);
  }

  switch (marker) {
    case 0xc0:
      return [null, offset];
    case 0xc2:
      return [false, offset];
    case 0xc3:
      return [true, offset];
    case 0xc4: {
      ensure(buffer, offset, 1);
      return readBinary(buffer, offset + 1, buffer[offset]);
    }
    case 0xc5: {
      ensure(buffer, offset, 2);
      return readBinary(buffer, offset + 2, buffer.readUInt16BE(offset));
    }
    case 0xc6: {
      ensure(buffer, offset, 4);
      return readBinary(buffer, offset + 4, buffer.readUInt32BE(offset));
    }
    case 0xca:
      ensure(buffer, offset, 4);
      return [buffer.readFloatBE(offset), offset + 4];
    case 0xcb:
      ensure(buffer, offset, 8);
      return [buffer.readDoubleBE(offset), offset + 8];
    case 0xcc:
      ensure(buffer, offset, 1);
      return [buffer[offset], offset + 1];
    case 0xcd:
      ensure(buffer, offset, 2);
      return [buffer.readUInt16BE(offset), offset + 2];
    case 0xce:
      ensure(buffer, offset, 4);
      return [buffer.readUInt32BE(offset), offset + 4];
    case 0xcf:
      return [readUInt64(buffer, offset), offset + 8];
    case 0xd0:
      ensure(buffer, offset, 1);
      return [buffer.readInt8(offset), offset + 1];
    case 0xd1:
      ensure(buffer, offset, 2);
      return [buffer.readInt16BE(offset), offset + 2];
    case 0xd2:
      ensure(buffer, offset, 4);
      return [buffer.readInt32BE(offset), offset + 4];
    case 0xd3:
      return [readInt64(buffer, offset), offset + 8];
    case 0xd9: {
      ensure(buffer, offset, 1);
      return readString(buffer, offset + 1, buffer[offset]);
    }
    case 0xda: {
      ensure(buffer, offset, 2);
      return readString(buffer, offset + 2, buffer.readUInt16BE(offset));
    }
    case 0xdb: {
      ensure(buffer, offset, 4);
      return readString(buffer, offset + 4, buffer.readUInt32BE(offset));
    }
    case 0xdc: {
      ensure(buffer, offset, 2);
      return readArray(buffer, offset + 2, buffer.readUInt16BE(offset));
    }
    case 0xdd: {
      ensure(buffer, offset, 4);
      return readArray(buffer, offset + 4, buffer.readUInt32BE(offset));
    }
    case 0xde: {
      ensure(buffer, offset, 2);
      return readMap(buffer, offset + 2, buffer.readUInt16BE(offset));
    }
    case 0xdf: {
      ensure(buffer, offset, 4);
      return readMap(buffer, offset + 4, buffer.readUInt32BE(offset));
    }
    default:
      throw new Error(`unsupported msgpack marker 0x${marker.toString(16)}`);
  }
}

function readBinary(buffer, offset, length) {
  ensure(buffer, offset, length);
  return [buffer.subarray(offset, offset + length), offset + length];
}

function readString(buffer, offset, length) {
  ensure(buffer, offset, length);
  return [buffer.toString('utf8', offset, offset + length), offset + length];
}

function readArray(buffer, offset, length) {
  const out = [];
  let cursor = offset;
  for (let index = 0; index < length; index += 1) {
    const [value, next] = decodeOne(buffer, cursor);
    out.push(value);
    cursor = next;
  }
  return [out, cursor];
}

function readMap(buffer, offset, length) {
  const out = {};
  let cursor = offset;
  for (let index = 0; index < length; index += 1) {
    const [key, afterKey] = decodeOne(buffer, cursor);
    const [value, afterValue] = decodeOne(buffer, afterKey);
    out[String(key)] = value;
    cursor = afterValue;
  }
  return [out, cursor];
}

function encode(value) {
  if (value === null || value === undefined) {
    return Buffer.from([0xc0]);
  }
  if (value === false) {
    return Buffer.from([0xc2]);
  }
  if (value === true) {
    return Buffer.from([0xc3]);
  }
  if (typeof value === 'number') {
    return encodeNumber(value);
  }
  if (typeof value === 'string') {
    return encodeString(value);
  }
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) {
    return encodeBinary(Buffer.from(value));
  }
  if (Array.isArray(value)) {
    return encodeArray(value);
  }
  if (typeof value === 'object') {
    return encodeMap(value);
  }
  throw new Error(`cannot msgpack encode ${typeof value}`);
}

function encodeNumber(value) {
  if (!Number.isSafeInteger(value)) {
    const out = Buffer.alloc(9);
    out[0] = 0xcb;
    out.writeDoubleBE(value, 1);
    return out;
  }
  if (value >= 0 && value <= 0x7f) {
    return Buffer.from([value]);
  }
  if (value < 0 && value >= -32) {
    return Buffer.from([0xe0 | (value + 32)]);
  }
  if (value >= 0 && value <= 0xff) {
    return Buffer.from([0xcc, value]);
  }
  if (value >= 0 && value <= 0xffff) {
    const out = Buffer.alloc(3);
    out[0] = 0xcd;
    out.writeUInt16BE(value, 1);
    return out;
  }
  if (value >= 0 && value <= 0xffffffff) {
    const out = Buffer.alloc(5);
    out[0] = 0xce;
    out.writeUInt32BE(value, 1);
    return out;
  }
  if (value >= -0x80 && value < 0) {
    const out = Buffer.alloc(2);
    out[0] = 0xd0;
    out.writeInt8(value, 1);
    return out;
  }
  if (value >= -0x8000 && value < 0) {
    const out = Buffer.alloc(3);
    out[0] = 0xd1;
    out.writeInt16BE(value, 1);
    return out;
  }
  if (value >= -0x80000000 && value < 0) {
    const out = Buffer.alloc(5);
    out[0] = 0xd2;
    out.writeInt32BE(value, 1);
    return out;
  }
  const out = Buffer.alloc(9);
  out[0] = value >= 0 ? 0xcf : 0xd3;
  value >= 0 ? out.writeBigUInt64BE(BigInt(value), 1) : out.writeBigInt64BE(BigInt(value), 1);
  return out;
}

function encodeString(value) {
  const bytes = Buffer.from(value, 'utf8');
  if (bytes.length < 32) {
    return Buffer.concat([Buffer.from([0xa0 + bytes.length]), bytes]);
  }
  if (bytes.length <= 0xff) {
    return Buffer.concat([Buffer.from([0xd9, bytes.length]), bytes]);
  }
  if (bytes.length <= 0xffff) {
    const header = Buffer.alloc(3);
    header[0] = 0xda;
    header.writeUInt16BE(bytes.length, 1);
    return Buffer.concat([header, bytes]);
  }
  const header = Buffer.alloc(5);
  header[0] = 0xdb;
  header.writeUInt32BE(bytes.length, 1);
  return Buffer.concat([header, bytes]);
}

function encodeBinary(bytes) {
  if (bytes.length <= 0xff) {
    return Buffer.concat([Buffer.from([0xc4, bytes.length]), bytes]);
  }
  if (bytes.length <= 0xffff) {
    const header = Buffer.alloc(3);
    header[0] = 0xc5;
    header.writeUInt16BE(bytes.length, 1);
    return Buffer.concat([header, bytes]);
  }
  const header = Buffer.alloc(5);
  header[0] = 0xc6;
  header.writeUInt32BE(bytes.length, 1);
  return Buffer.concat([header, bytes]);
}

function encodeArray(values) {
  const body = values.map(encode);
  if (values.length < 16) {
    return Buffer.concat([Buffer.from([0x90 + values.length]), ...body]);
  }
  const header = Buffer.alloc(3);
  header[0] = 0xdc;
  header.writeUInt16BE(values.length, 1);
  return Buffer.concat([header, ...body]);
}

function encodeMap(value) {
  const entries = Object.entries(value).filter((entry) => entry[1] !== undefined);
  const body = entries.flatMap(([key, item]) => [encodeString(key), encode(item)]);
  if (entries.length < 16) {
    return Buffer.concat([Buffer.from([0x80 + entries.length]), ...body]);
  }
  const header = Buffer.alloc(3);
  header[0] = 0xde;
  header.writeUInt16BE(entries.length, 1);
  return Buffer.concat([header, ...body]);
}

class RemoteStream {
  constructor(id, connection) {
    this.id = id;
    this.connection = connection;
    this.chunks = [];
    this.ended = false;
    this.waiters = [];
  }

  pushData(segment) {
    this.chunks.push(Buffer.from(segment));
  }

  markEnded() {
    this.ended = true;
    const data = Buffer.concat(this.chunks);
    for (const waiter of this.waiters) {
      waiter(data);
    }
    this.waiters = [];
  }

  collect() {
    if (this.ended) {
      return Promise.resolve(Buffer.concat(this.chunks));
    }
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  end() {
    this.connection.notify('stream_ended', { stream: this.id });
  }
}

class RpcConnection {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.buffer = Buffer.alloc(0);
    this.callbacks = new Map();
    this.streams = new Map();
    socket.on('data', (chunk) => this.onData(chunk));
    socket.once('close', () => this.rejectAll(new Error('connection closed')));
    socket.once('error', (error) => this.rejectAll(error));
  }

  call(method, params = {}, onStream, timeoutMs = 15000) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.callbacks.delete(id);
        reject(new Error(`timeout calling ${method}`));
      }, timeoutMs);
      this.callbacks.set(id, {
        onStream,
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.write({ id, method, params });
    }).finally(() => this.callbacks.delete(id));
  }

  notify(method, params = {}) {
    this.write({ method, params });
  }

  write(message) {
    this.socket.write(encode(message));
  }

  onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    let offset = 0;
    while (offset < this.buffer.length) {
      try {
        const [message, next] = decodeOne(this.buffer, offset);
        this.dispatch(message);
        offset = next;
      } catch (error) {
        if (error === NEED_MORE) {
          break;
        }
        throw error;
      }
    }
    this.buffer = this.buffer.subarray(offset);
  }

  dispatch(message) {
    if (!message || typeof message !== 'object') {
      return;
    }
    if ('result' in message || 'error' in message) {
      const callback = this.callbacks.get(message.id);
      if (!callback) {
        return;
      }
      if ('error' in message) {
        callback.reject(new Error(message.error && message.error.message ? message.error.message : 'rpc error'));
      } else {
        callback.resolve(message.result);
      }
      return;
    }
    if (message.method === 'streams_started') {
      const params = message.params || {};
      const callback = this.callbacks.get(params.for_request_id);
      if (!callback || !callback.onStream) {
        return;
      }
      for (const streamId of params.stream_ids || []) {
        const stream = new RemoteStream(streamId, this);
        this.streams.set(streamId, stream);
        callback.onStream(stream);
      }
      return;
    }
    if (message.method === 'stream_data') {
      const params = message.params || {};
      this.streams.get(params.stream)?.pushData(params.segment || Buffer.alloc(0));
      return;
    }
    if (message.method === 'stream_ended') {
      const params = message.params || {};
      const stream = this.streams.get(params.stream);
      if (stream) {
        stream.markEnded();
        this.streams.delete(params.stream);
      }
    }
  }

  rejectAll(error) {
    for (const callback of this.callbacks.values()) {
      callback.reject(error);
    }
    this.callbacks.clear();
  }
}

function withTimeout(promise, timeoutMs, label) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${label}`)), timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function connect(host, port, timeoutMs) {
  return new Promise((resolve, reject) => {
    const socket = net.connect({ host, port });
    const timer = setTimeout(() => {
      socket.destroy();
      reject(new Error(`timeout connecting to ${host}:${port}`));
    }, timeoutMs);
    socket.once('connect', () => {
      clearTimeout(timer);
      resolve(socket);
    });
    socket.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

async function spawnAndCapture(connection, command, args, options, timeoutMs) {
  const streams = [];
  const streamsReady = new Promise((resolve) => {
    const onStream = (stream) => {
      streams.push(stream);
      if (streams.length >= 3) {
        resolve(streams);
      }
    };
    const params = {
      command,
      args,
      env: options.env || {},
      cwd: options.cwd,
    };
    streams.exitPromise = connection.call('spawn', params, onStream, timeoutMs);
  });

  const [stdin, stdout, stderr] = await withTimeout(streamsReady, Math.min(timeoutMs, 5000), 'spawn streams');
  stdin.end();
  const stdoutPromise = stdout.collect();
  const stderrPromise = stderr.collect();
  const exit = await streams.exitPromise;
  const stdoutData = await withTimeout(stdoutPromise, 3000, 'stdout end').catch(() => Buffer.concat(stdout.chunks));
  const stderrData = await withTimeout(stderrPromise, 3000, 'stderr end').catch(() => Buffer.concat(stderr.chunks));
  return {
    exitCode: exit && typeof exit.exit_code === 'number' ? exit.exit_code : null,
    message: exit && exit.message ? String(exit.message) : '',
    stdout: stdoutData.toString('utf8'),
    stderr: stderrData.toString('utf8'),
  };
}

async function main() {
  const input = JSON.parse(process.argv[2] || '{}');
  const host = input.host || '127.0.0.1';
  const port = Number(input.port);
  const timeoutMs = Number(input.timeoutMs || 15000);
  if (!port) {
    throw new Error('missing port');
  }
  if (!input.token) {
    throw new Error('missing exec server token');
  }

  const socket = await connect(host, port, timeoutMs);
  const connection = new RpcConnection(socket);
  const vsdaPath = input.vsdaPath || process.env.AGENT_PILOT_VSDA_PATH || '/Applications/Visual Studio Code.app/Contents/Resources/app/node_modules/vsda';
  const vsda = require(vsdaPath);
  const Signer = vsda.signer || vsda.Signer;
  if (!Signer) {
    throw new Error('VS Code signer is unavailable');
  }

  const challengeResult = await connection.call('challenge_issue', { token: input.token }, undefined, timeoutMs);
  const response = new Signer().sign(challengeResult.challenge);
  await connection.call('challenge_verify', { response }, undefined, timeoutMs);

  let remoteEnv = null;
  try {
    remoteEnv = await connection.call('get_env', {}, undefined, timeoutMs);
  } catch (_) {
    remoteEnv = null;
  }

  const command = input.command || 'sh';
  const args = Array.isArray(input.args) ? input.args : ['-c', input.script || ''];
  const result = await spawnAndCapture(connection, command, args, { cwd: input.cwd, env: input.env }, timeoutMs);
  socket.end();

  process.stdout.write(JSON.stringify({ ok: true, env: remoteEnv, result }));
}

main().catch((error) => {
  process.stdout.write(JSON.stringify({ ok: false, error: error && error.stack ? error.stack : String(error) }));
  process.exit(0);
});
