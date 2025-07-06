import http from "k6/http";

export const options = {
  discardResponseBodies: true,

  scenarios: {
    contacts: {
      executor: 'ramping-arrival-rate',

      // Start iterations per `timeUnit`
      startRate: 1000,

      // Start `startRate` iterations per minute
      timeUnit: '1s',

      // Pre-allocate necessary VUs.
      preAllocatedVUs: 1000,

      stages: [
        { target: 20000, duration: '1s' },
        // { target: 2000, duration: '10s' },
        // { target: 5000, duration: '10s' },
        // { target: 10000, duration: '10s' },
        // { target: 20000, duration: '10s' },
        { target: 20000, duration: '20s' },
      ],
    },
  },
};
const params = {
  timeout: '3s'
}
export default function () {
  http.get('http://127.0.0.1:3001', params);
}