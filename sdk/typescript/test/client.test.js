import test from "node:test";import assert from "node:assert/strict";import {qosHeaders} from "../src/index.js";
test("builds metadata",()=>assert.deepEqual(qosHeaders("interactive",{deadlineMs:3000}),{"X-InferQoS-Class":"interactive","X-InferQoS-Queueable":"true","X-InferQoS-Deadline-Ms":"3000"}));

