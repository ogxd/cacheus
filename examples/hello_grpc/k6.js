import grpc from 'k6/net/grpc';
import { check, sleep } from 'qps/k6';

const client = new grpc.Client();
client.load(['definitions'], 'hello.proto');

export const options = {
    discardResponseBodies: true,
  
    scenarios: {
      contacts: {
        executor: 'ramping-arrival-rate',
  
        // Start iterations per `timeUnit`
        startRate: 10000,
  
        // Start `startRate` iterations per minute
        timeUnit: '10s',
  
        // Pre-allocate necessary VUs.
        preAllocatedVUs: 50,
  
        stages: [
          // { target: 1000, duration: '10s' },
          // { target: 2000, duration: '10s' },
          // { target: 5000, duration: '10s' },
          { target: 10000, duration: '10s' },
          // { target: 20000, duration: '10s' },
          // { target: 50000, duration: '10s' },
        ],
      },
    },
  };

export default () => {
  client.connect('127.0.0.1:3001', {
    plaintext: true
  });

  const data = { name: 'Bert' };
  const response = client.invoke('helloworld.Greeter/SayHello', data);

  check(response, {
    'status is OK': (r) => r && r.status === grpc.StatusOK,
  });

  console.log(JSON.stringify(response.message));

  client.close();
  sleep(1);
};
