// Small application-owned 3D math module, column-major matrices; not a Runtime graphics engine.
export function multiply(a,b){const out=new Float32Array(16);for(let c=0;c<4;c++)for(let r=0;r<4;r++)for(let k=0;k<4;k++)out[c*4+r]+=a[k*4+r]*b[c*4+k];return out;}
export function identity(){return new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]);}
export function translate(x,y,z){const m=identity();m[12]=x;m[13]=y;m[14]=z;return m;}
export function scale(x,y,z){const m=identity();m[0]=x;m[5]=y;m[10]=z;return m;}
export function rotateX(a){const m=identity(),c=Math.cos(a),s=Math.sin(a);m[5]=c;m[6]=s;m[9]=-s;m[10]=c;return m;}
export function rotateY(a){const m=identity(),c=Math.cos(a),s=Math.sin(a);m[0]=c;m[2]=-s;m[8]=s;m[10]=c;return m;}
export function perspective(aspect){if(!Number.isFinite(aspect)||aspect<=0)throw new RangeError('aspect');const f=1/Math.tan(Math.PI/8),n=.1,z=100;return new Float32Array([f/aspect,0,0,0,0,f,0,0,0,0,(z+n)/(n-z),-1,0,0,2*z*n/(n-z),0]);}
export function initial(){return {yaw:.6,pitch:.55,distance:12,motion:true,explode:false,selected:0,time:0,revision:0};}
export function command(state,action,revision=state.revision){if(revision!==state.revision)return false;switch(action){case'motion':state.motion=!state.motion;break;case'step':break;case'select':state.selected=(state.selected+1)%6;break;case'explode':state.explode=!state.explode;break;case'near':state.distance=Math.max(7,state.distance-1);break;case'far':state.distance=Math.min(20,state.distance+1);break;case'reset':Object.assign(state,initial());break;default:return false;}state.revision=revision+1;return true;}
// Lorenz equations, classic parameters: sigma=10, rho=28, beta=8/3.
// Reference: MIT 8.09 Classical Mechanics III, Fall 2014, problem set 9.
export const TRAILS=6, TRAIL_LENGTH=4096, FIXED_STEP=.005;
export function derivative([x,y,z]){return [10*(y-x),x*(28-z)-y,x*y-(8/3)*z];}
export function rk4(point,dt=FIXED_STEP){
 if(!Number.isFinite(dt)||dt<=0||dt>.01||point.length!==3||!point.every(Number.isFinite))throw new RangeError('bounded Lorenz step');
 const k1=derivative(point),k2=derivative(point.map((p,i)=>p+k1[i]*dt/2)),k3=derivative(point.map((p,i)=>p+k2[i]*dt/2)),k4=derivative(point.map((p,i)=>p+k3[i]*dt));
 const next=point.map((p,i)=>p+dt*(k1[i]+2*k2[i]+2*k3[i]+k4[i])/6);
 if(next.some(p=>!Number.isFinite(p)||Math.abs(p)>200))throw new RangeError('Lorenz divergence');return next;
}
export function createLorenz(){
 const simulation={points:new Float32Array(TRAILS*TRAIL_LENGTH*3),heads:Array.from({length:TRAILS},(_,i)=>[1+i*.001,1,1]),cursor:0,count:0,steps:0};
 // Bounded warm start reveals both wings immediately; this is example-owned initial data.
 for(let i=0;i<256;i++)advanceLorenz(simulation,8);return simulation;
}
export function advanceLorenz(simulation,steps){
 if(!Number.isInteger(steps)||steps<0||steps>8)throw new RangeError('per-tick integration budget');
 for(let s=0;s<steps;s++){
  for(let i=0;i<TRAILS;i++){const p=rk4(simulation.heads[i]);simulation.heads[i]=p;const offset=(i*TRAIL_LENGTH+simulation.cursor)*3;simulation.points[offset]=p[0]*.1;simulation.points[offset+1]=(p[2]-25)*.1;simulation.points[offset+2]=p[1]*.1;}
  simulation.cursor=(simulation.cursor+1)%TRAIL_LENGTH;simulation.count=Math.min(TRAIL_LENGTH,simulation.count+1);simulation.steps++;
 }
}
